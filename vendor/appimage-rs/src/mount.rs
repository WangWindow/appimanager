//! Pure-Rust FUSE mount of the embedded SquashFS (Linux only).

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use backhand::{FilesystemReader, InnerNode, NodeHeader, SquashfsFileReader};
use polyfuse::{
    op::Operation,
    reply::{AttrOut, EntryOut, OpenOut, ReaddirOut, StatfsOut},
    KernelConfig, Request, Session,
};

use crate::error::{Error, Result};

const ROOT_INO: u64 = 1;
const TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u32 = 512;

/// A mounted FUSE session.  Dropping it unmounts the filesystem and waits for
/// its request-serving thread to stop.
pub(crate) struct BackgroundSession {
    _session: Arc<Session>,
    worker: Option<JoinHandle<()>>,
    mountpoint: PathBuf,
    fusermount: PathBuf,
}

impl Drop for BackgroundSession {
    fn drop(&mut self) {
        // This wakes `Session::next_request` with ENODEV, allowing the worker
        // to release its Session reference before we join it.
        let _ = Command::new(&self.fusermount)
            .args(["-u", "-q", "-z", "--"])
            .arg(&self.mountpoint)
            .status();

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // `Session` then drops its FUSE connection.  polyfuse's own best-effort
        // unmount is harmless if the explicit unmount above already succeeded.
    }
}

/// Mount the SquashFS at `offset` inside `image` on `mountpoint`.
pub(crate) fn mount(image: &Path, offset: u64, mountpoint: &Path) -> Result<BackgroundSession> {
    let reader = BufReader::new(File::open(image)?);
    let filesystem = Arc::new(SquashFuse::index(crate::compression::open_reader(
        reader, offset,
    )?));
    let fusermount = fusermount_path().ok_or_else(|| {
        Error::MountFailed("could not find fusermount3 or fusermount in PATH".to_owned())
    })?;

    let mut config = KernelConfig::default();
    config
        .fusermount_path(&fusermount)
        .mount_option("ro")
        .mount_option("fsname=squashfuse")
        .mount_option("subtype=appimage");

    let session = Arc::new(
        Session::mount(mountpoint.to_path_buf(), config)
            .map_err(|error| Error::MountFailed(error.to_string()))?,
    );
    let worker_session = Arc::clone(&session);
    let worker_filesystem = Arc::clone(&filesystem);
    let worker = thread::Builder::new()
        .name("appimage-fuse".to_owned())
        .spawn(move || serve(worker_session, worker_filesystem))
        .map_err(|error| Error::MountFailed(error.to_string()))?;

    Ok(BackgroundSession {
        _session: session,
        worker: Some(worker),
        mountpoint: mountpoint.to_path_buf(),
        fusermount,
    })
}

fn fusermount_path() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&paths) {
        for program in ["fusermount3", "fusermount"] {
            let path = directory.join(program);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn serve(session: Arc<Session>, filesystem: Arc<SquashFuse>) {
    loop {
        let request = match session.next_request() {
            Ok(Some(request)) => request,
            Ok(None) | Err(_) => return,
        };
        let _ = filesystem.handle(&request);
    }
}

/// One directory entry as seen by `readdir` / `lookup`.
struct Child {
    name: OsString,
    ino: u64,
    kind: FileKind,
}

#[derive(Clone, Copy)]
enum FileKind {
    Directory,
    RegularFile,
    Symlink,
    CharDevice,
    BlockDevice,
    NamedPipe,
    Socket,
}

impl From<FileKind> for u32 {
    fn from(kind: FileKind) -> Self {
        match kind {
            FileKind::Directory => libc::S_IFDIR,
            FileKind::RegularFile => libc::S_IFREG,
            FileKind::Symlink => libc::S_IFLNK,
            FileKind::CharDevice => libc::S_IFCHR,
            FileKind::BlockDevice => libc::S_IFBLK,
            FileKind::NamedPipe => libc::S_IFIFO,
            FileKind::Socket => libc::S_IFSOCK,
        }
    }
}

impl From<FileKind> for u8 {
    fn from(kind: FileKind) -> Self {
        match kind {
            FileKind::Directory => libc::DT_DIR,
            FileKind::RegularFile => libc::DT_REG,
            FileKind::Symlink => libc::DT_LNK,
            FileKind::CharDevice => libc::DT_CHR,
            FileKind::BlockDevice => libc::DT_BLK,
            FileKind::NamedPipe => libc::DT_FIFO,
            FileKind::Socket => libc::DT_SOCK,
        }
    }
}

struct Attr {
    kind: FileKind,
    size: u64,
    perm: u32,
    uid: u32,
    gid: u32,
    mtime: Duration,
}

/// Kind-specific payload for an inode.
enum Body {
    Dir(Vec<Child>),
    File(SquashfsFileReader),
    Symlink(Vec<u8>),
    Other,
}

struct Inode {
    attr: Attr,
    parent: u64,
    body: Body,
}

/// A read-only FUSE view over a decoded SquashFS.
struct SquashFuse {
    filesystem: FilesystemReader<'static>,
    /// Indexed by `ino - 1`; `inodes[0]` is the root directory.
    inodes: Vec<Inode>,
    /// Single-entry cache of a fully decompressed file, keyed by inode.
    cache: Mutex<Option<(u64, Arc<[u8]>)>>,
}

impl SquashFuse {
    fn index(filesystem: FilesystemReader<'static>) -> Self {
        let mut inodes = vec![Inode {
            attr: build_attr(&NodeHeader::new(0o755, 0, 0, 0), FileKind::Directory, 0),
            parent: ROOT_INO,
            body: Body::Dir(Vec::new()),
        }];
        let mut ino_of = HashMap::from([(PathBuf::from("/"), ROOT_INO)]);

        // First pass: assign an inode number to every path.
        for node in filesystem.files() {
            if node.fullpath.as_os_str() == "/" {
                inodes[0].attr = attr_for(&node.header, &node.inner);
                continue;
            }
            let ino = inodes.len() as u64 + 1;
            ino_of.insert(node.fullpath.clone(), ino);
            inodes.push(Inode {
                attr: attr_for(&node.header, &node.inner),
                parent: ROOT_INO, // fixed up below
                body: body_for(&node.inner),
            });
        }

        // Second pass: link each inode into its parent directory.
        for node in filesystem.files() {
            let path = node.fullpath.as_path();
            if path == Path::new("/") {
                continue;
            }

            let ino = ino_of[path];
            let parent = path
                .parent()
                .and_then(|path| ino_of.get(path).copied())
                .unwrap_or(ROOT_INO);

            let name = path.file_name().unwrap_or_default().to_os_string();
            let kind = inodes[ino as usize - 1].attr.kind;

            inodes[ino as usize - 1].parent = parent;

            if let Body::Dir(children) = &mut inodes[parent as usize - 1].body {
                children.push(Child { name, ino, kind });
            }
        }

        Self {
            filesystem,
            inodes,
            cache: Mutex::new(None),
        }
    }

    #[inline]
    fn inode(&self, ino: u64) -> Option<&Inode> {
        self.inodes.get((ino as usize).checked_sub(1)?)
    }

    /// Fully decompressed bytes of a file inode, cached for repeat reads.
    fn file_bytes(&self, ino: u64, file: &SquashfsFileReader) -> std::io::Result<Arc<[u8]>> {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some((cached_ino, bytes)) = cache.as_ref() {
            if *cached_ino == ino {
                return Ok(Arc::clone(bytes));
            }
        }

        let mut buf = Vec::with_capacity(file.file_len());
        self.filesystem
            .file(file)
            .reader()
            .read_to_end(&mut buf)
            .map_err(|error| std::io::Error::other(error.to_string()))?;

        let bytes: Arc<[u8]> = Arc::from(buf);
        *cache = Some((ino, Arc::clone(&bytes)));
        Ok(bytes)
    }

    fn handle(&self, request: &Request) -> std::io::Result<()> {
        let operation = match request.operation() {
            Ok(operation) => operation,
            Err(_) => return request.reply_error(libc::EIO),
        };

        match operation {
            Operation::Lookup(operation) => {
                self.lookup(request, operation.parent(), operation.name())
            }
            Operation::Getattr(operation) => self.getattr(request, operation.ino()),
            Operation::Readlink(operation) => self.readlink(request, operation.ino()),
            Operation::Open(operation) => self.open(request, operation.ino(), false),
            Operation::Opendir(operation) => self.open(request, operation.ino(), true),
            Operation::Read(operation) => self.read(
                request,
                operation.ino(),
                operation.offset(),
                operation.size(),
            ),
            Operation::Readdir(operation) => self.readdir(
                request,
                operation.ino(),
                operation.offset(),
                operation.size(),
            ),
            Operation::Release(_)
            | Operation::Releasedir(_)
            | Operation::Flush(_)
            | Operation::Fsync(_)
            | Operation::Fsyncdir(_) => request.reply(()),
            Operation::Access(_) => request.reply(()),
            Operation::Statfs(_) => self.statfs(request),
            // Forget requests deliberately have no reply in the FUSE protocol.
            Operation::Forget(_) | Operation::Interrupt(_) => Ok(()),
            Operation::Setattr(_)
            | Operation::Write(_, _)
            | Operation::Create(_)
            | Operation::Mknod(_)
            | Operation::Mkdir(_)
            | Operation::Unlink(_)
            | Operation::Rmdir(_)
            | Operation::Rename(_)
            | Operation::Link(_)
            | Operation::Symlink(_)
            | Operation::Removexattr(_)
            | Operation::Fallocate(_)
            | Operation::CopyFileRange(_) => request.reply_error(libc::EROFS),
            Operation::Getxattr(_) | Operation::Listxattr(_) => request.reply_error(libc::ENODATA),
            _ => request.reply_error(libc::ENOSYS),
        }
    }

    fn lookup(&self, request: &Request, parent: u64, name: &OsStr) -> std::io::Result<()> {
        let Some(Inode {
            body: Body::Dir(children),
            ..
        }) = self.inode(parent)
        else {
            return request.reply_error(libc::ENOTDIR);
        };
        let Some(child) = children.iter().find(|child| child.name == name) else {
            return request.reply_error(libc::ENOENT);
        };
        let mut out = EntryOut::default();
        out.ino(child.ino);
        out.generation(0);
        out.ttl_attr(TTL);
        out.ttl_entry(TTL);
        fill_attr(
            out.attr(),
            child.ino,
            &self.inode(child.ino).expect("indexed child").attr,
        );
        request.reply(out)
    }

    fn getattr(&self, request: &Request, ino: u64) -> std::io::Result<()> {
        let Some(inode) = self.inode(ino) else {
            return request.reply_error(libc::ENOENT);
        };
        let mut out = AttrOut::default();
        out.ttl(TTL);
        fill_attr(out.attr(), ino, &inode.attr);
        request.reply(out)
    }

    fn readlink(&self, request: &Request, ino: u64) -> std::io::Result<()> {
        match self.inode(ino).map(|inode| &inode.body) {
            Some(Body::Symlink(target)) => request.reply(target.as_slice()),
            Some(_) => request.reply_error(libc::EINVAL),
            None => request.reply_error(libc::ENOENT),
        }
    }

    fn open(&self, request: &Request, ino: u64, directory: bool) -> std::io::Result<()> {
        let Some(inode) = self.inode(ino) else {
            return request.reply_error(libc::ENOENT);
        };
        match (&inode.body, directory) {
            (Body::Dir(_), true) | (Body::File(_), false) => {
                let mut out = OpenOut::default();
                out.fh(ino);
                request.reply(out)
            }
            (Body::Dir(_), false) => request.reply_error(libc::EISDIR),
            _ => request.reply_error(libc::ENOTDIR),
        }
    }

    fn read(&self, request: &Request, ino: u64, offset: u64, size: u32) -> std::io::Result<()> {
        let Some(Inode {
            body: Body::File(file),
            ..
        }) = self.inode(ino)
        else {
            return request.reply_error(libc::EISDIR);
        };
        match self.file_bytes(ino, file) {
            Ok(bytes) => {
                let start = (offset as usize).min(bytes.len());
                let end = start.saturating_add(size as usize).min(bytes.len());
                request.reply(&bytes[start..end])
            }
            Err(_) => request.reply_error(libc::EIO),
        }
    }

    fn readdir(&self, request: &Request, ino: u64, offset: u64, size: u32) -> std::io::Result<()> {
        let Some(Inode {
            parent,
            body: Body::Dir(children),
            ..
        }) = self.inode(ino)
        else {
            return request.reply_error(libc::ENOTDIR);
        };
        let mut out = ReaddirOut::new(size as usize);
        let entries = [
            (ino, FileKind::Directory, OsStr::new(".")),
            (*parent, FileKind::Directory, OsStr::new("..")),
        ]
        .into_iter()
        .chain(
            children
                .iter()
                .map(|child| (child.ino, child.kind, child.name.as_os_str())),
        )
        .enumerate()
        .skip(offset as usize);
        for (index, (child_ino, kind, name)) in entries {
            let kind: u8 = kind.into();
            if out.entry(name, child_ino, kind.into(), (index + 1) as u64) {
                break;
            }
        }
        request.reply(out)
    }

    fn statfs(&self, request: &Request) -> std::io::Result<()> {
        let mut out = StatfsOut::default();
        let statfs = out.statfs();
        statfs.bsize(BLOCK_SIZE);
        statfs.frsize(BLOCK_SIZE);
        statfs.files(self.inodes.len() as u64);
        statfs.ffree(0);
        request.reply(out)
    }
}

fn body_for(inner: &InnerNode<SquashfsFileReader>) -> Body {
    match inner {
        InnerNode::Dir(_) => Body::Dir(Vec::new()),
        InnerNode::File(file) => Body::File(file.clone()),
        InnerNode::Symlink(symlink) => Body::Symlink(symlink.link.as_os_str().as_bytes().to_vec()),
        _ => Body::Other,
    }
}

fn attr_for(header: &NodeHeader, inner: &InnerNode<SquashfsFileReader>) -> Attr {
    let (kind, size) = match inner {
        InnerNode::Dir(_) => (FileKind::Directory, 0),
        InnerNode::File(file) => (FileKind::RegularFile, file.file_len() as u64),
        InnerNode::Symlink(symlink) => (
            FileKind::Symlink,
            symlink.link.as_os_str().as_bytes().len() as u64,
        ),
        InnerNode::CharacterDevice(_) => (FileKind::CharDevice, 0),
        InnerNode::BlockDevice(_) => (FileKind::BlockDevice, 0),
        InnerNode::NamedPipe => (FileKind::NamedPipe, 0),
        InnerNode::Socket => (FileKind::Socket, 0),
    };
    build_attr(header, kind, size)
}

fn build_attr(header: &NodeHeader, kind: FileKind, size: u64) -> Attr {
    Attr {
        kind,
        size,
        perm: u32::from(header.permissions & 0o7777),
        uid: header.uid,
        gid: header.gid,
        mtime: Duration::from_secs(header.mtime.into()),
    }
}

fn fill_attr(target: &mut polyfuse::reply::FileAttr, ino: u64, attr: &Attr) {
    target.ino(ino);
    target.size(attr.size);
    target.blocks(attr.size.div_ceil(u64::from(BLOCK_SIZE)));
    target.atime(attr.mtime);
    target.mtime(attr.mtime);
    target.ctime(attr.mtime);
    let kind: u32 = attr.kind.into();
    target.mode(kind | attr.perm);
    target.nlink(if matches!(attr.kind, FileKind::Directory) {
        2
    } else {
        1
    });
    target.uid(attr.uid);
    target.gid(attr.gid);
    target.rdev(0);
    target.blksize(BLOCK_SIZE);
}
