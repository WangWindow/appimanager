//! Builds a self-contained type-2 AppImage for the integration tests: a real
//! SquashFS payload (written by `backhand`) appended to a hand-crafted ELF64
//! whose computed size lands exactly on the payload — so `AppImage::open`
//! reports the right offset and the sections/tree are genuinely parsed.

#![allow(dead_code)]

use std::io::Cursor;
use std::path::PathBuf;

use backhand::compression::Compressor;
use backhand::{FilesystemCompressor, FilesystemWriter, NodeHeader};
use tempfile::TempDir;

/// A written-to-disk synthetic AppImage plus the values tests assert against.
pub struct Fixture {
    pub dir: TempDir,
    pub path: PathBuf,
    /// Offset where the SquashFS payload begins (== the ELF's own size).
    pub offset: u64,
    pub upd_info: &'static str,
}

pub const UPD_INFO: &str = "zsync|https://example.test/App-latest.zsync";
pub const DATA_CONTENTS: &str = "payload\n";
pub const NESTED_CONTENTS: &str = "nested payload\n";
pub const APPRUN_CONTENTS: &str = "#!/bin/sh\nexec true\n";
/// mtime stamped into the gzip fixture's nodes (2020-09-13T12:26:40Z).
pub const MTIME: u32 = 1_600_000_000;

impl Fixture {
    /// Write a fresh gzip-compressed AppImage into a new temp dir.
    pub fn new() -> Self {
        Self::wrap(&build_squashfs())
    }

    /// Wrap an arbitrary SquashFS payload (e.g. a committed xz/zstd fixture) in a
    /// synthetic ELF and write the resulting AppImage to a new temp dir.
    pub fn wrap(squashfs: &[u8]) -> Self {
        let elf = build_elf(&[(".upd_info", UPD_INFO.as_bytes()), (".sha256_sig", b"")]);
        let offset = elf.len() as u64;

        let mut bytes = elf;
        bytes.extend_from_slice(squashfs);

        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("App.AppImage");
        std::fs::write(&path, &bytes).expect("write appimage");

        Self {
            dir,
            path,
            offset,
            upd_info: UPD_INFO,
        }
    }
}

/// A minimal valid runtime ELF suitable for [`appimage::AppImage::pack`] — its
/// computed size equals its byte length, so an appended payload lands right.
pub fn runtime_elf() -> Vec<u8> {
    build_elf(&[(".upd_info", UPD_INFO.as_bytes()), (".sha256_sig", b"")])
}

/// A SquashFS with a small tree: `AppRun`, `data`, `dir/nested`, and a
/// `symlink -> data`.
fn build_squashfs() -> Vec<u8> {
    let mut writer = FilesystemWriter::default();
    writer.set_compressor(
        FilesystemCompressor::new(Compressor::Gzip, None).expect("gzip compressor"),
    );

    let dir = NodeHeader::new(0o755, 0, 0, MTIME);
    let exe = NodeHeader::new(0o755, 0, 0, MTIME);
    let file = NodeHeader::new(0o644, 0, 0, MTIME);
    let link = NodeHeader::new(0o777, 0, 0, MTIME);

    writer
        .push_file(Cursor::new(APPRUN_CONTENTS), "AppRun", exe)
        .unwrap();
    writer
        .push_file(Cursor::new(DATA_CONTENTS), "data", file)
        .unwrap();
    writer.push_dir("dir", dir).unwrap();
    writer
        .push_file(Cursor::new(NESTED_CONTENTS), "dir/nested", file)
        .unwrap();
    writer.push_symlink("data", "symlink", link).unwrap();

    let mut out = Cursor::new(Vec::new());
    writer.write(&mut out).expect("write squashfs");
    out.into_inner()
}

/// A minimal but valid ELF64 little-endian file whose section-header table sits
/// at the very end, so `appimage_get_elf_size` == the file length.
fn build_elf(sections: &[(&str, &[u8])]) -> Vec<u8> {
    const EHDR: usize = 64;
    const SHDR: usize = 64;

    // Section-name string table: leading NUL, then each name, then ".shstrtab".
    let mut shstrtab = vec![0u8];
    let mut name_offsets = Vec::new();
    for (name, _) in sections {
        name_offsets.push(shstrtab.len() as u32);
        shstrtab.extend_from_slice(name.as_bytes());
        shstrtab.push(0);
    }
    let shstrtab_name = shstrtab.len() as u32;
    shstrtab.extend_from_slice(b".shstrtab\0");

    // Section data laid out right after the ELF header.
    let mut body = Vec::new();
    let mut shdrs = vec![Shdr::default()]; // index 0: SHT_NULL
    for ((_, data), &name) in sections.iter().zip(&name_offsets) {
        let offset = (EHDR + body.len()) as u64;
        body.extend_from_slice(data);
        shdrs.push(Shdr {
            name,
            kind: 1, // SHT_PROGBITS
            offset,
            size: data.len() as u64,
        });
    }
    let shstrtab_offset = (EHDR + body.len()) as u64;
    body.extend_from_slice(&shstrtab);
    let shstrndx = shdrs.len() as u16;
    shdrs.push(Shdr {
        name: shstrtab_name,
        kind: 3, // SHT_STRTAB
        offset: shstrtab_offset,
        size: shstrtab.len() as u64,
    });

    let e_shoff = (EHDR + body.len()) as u64;
    let e_shnum = shdrs.len() as u16;

    let mut out = vec![0u8; EHDR];
    out[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
    out[4] = 2; // ELFCLASS64
    out[5] = 1; // ELFDATA2LSB
    out[6] = 1; // EV_CURRENT
    out[16..18].copy_from_slice(&2u16.to_le_bytes()); // e_type = ET_EXEC
    out[18..20].copy_from_slice(&0x3eu16.to_le_bytes()); // e_machine = x86-64
    out[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
    out[40..48].copy_from_slice(&e_shoff.to_le_bytes()); // e_shoff
    out[52..54].copy_from_slice(&(EHDR as u16).to_le_bytes()); // e_ehsize
    out[58..60].copy_from_slice(&(SHDR as u16).to_le_bytes()); // e_shentsize
    out[60..62].copy_from_slice(&e_shnum.to_le_bytes()); // e_shnum
    out[62..64].copy_from_slice(&shstrndx.to_le_bytes()); // e_shstrndx

    out.extend_from_slice(&body);
    for shdr in &shdrs {
        out.extend_from_slice(&shdr.encode());
    }
    out
}

#[derive(Default)]
struct Shdr {
    name: u32,
    kind: u32,
    offset: u64,
    size: u64,
}

impl Shdr {
    fn encode(&self) -> [u8; 64] {
        let mut buf = [0u8; 64];
        buf[0..4].copy_from_slice(&self.name.to_le_bytes());
        buf[4..8].copy_from_slice(&self.kind.to_le_bytes());
        // sh_flags(8), sh_addr(16) left zero
        buf[24..32].copy_from_slice(&self.offset.to_le_bytes());
        buf[32..40].copy_from_slice(&self.size.to_le_bytes());
        // sh_link(40), sh_info(44) zero
        buf[48..56].copy_from_slice(&1u64.to_le_bytes()); // sh_addralign
        buf
    }
}
