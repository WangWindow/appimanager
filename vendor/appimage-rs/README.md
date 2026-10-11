# appimage

This is a minimal, entirely in Rust, reader/extractor/mounter for everyone's favorite type-2 AppImages.
It's main goal is to provide an alternative for the official [AppImageRuntime](https://github.com/AppImage/type2-runtime) for embedding and library use for Rust projects.

Minimal as-in, we're lucky type-2 appimages are just ELF's with SquashFS appended (to simplify things); so this crate is more or less an ergonomic wrapper around [`object`](https://crates.io/crates/object) + [`memmap2`](https://crates.io/crates/memmap2) + [`backhand`](https://crates.io/crates/backhand) + [`polyfuse`](https://crates.io/crates/polyfuse).

For all platforms: you can read, and you can open, and you can unpack.
For linux: All of that and you can mount things too.

Linux mounting uses the system `fusermount3` (or `fusermount`) helper and
`/dev/fuse` at runtime, but does not build or link against `libfuse`.

## Reading and unpacking

```rust,ignore
use appimage::{AppImage, Payload, Temp};

let image = AppImage::open("App.AppImage")?;
println!("squashfs at byte {}", image.offset());

// ELF sections used by the AppImage spec.
if let Some(info) = image.update_information()? {
    println!("update info: {}", info.to_string_lossy());
}

// Unpack the whole payload to a directory you own!
let dir = image.unpack().to("out/AppDir")?;
assert!(dir.has_app_run());

// a subset by fnmatch pattern
image.unpack().matching("usr/lib/*").to("out/libs")?;
```

## Update information

`.upd_info` is parsed into a typed [`UpdateInformation`] (the raw bytes are still
available via `update_information()`):

```rust,ignore
match image.update_info()? {
    Some(appimage::UpdateInformation::GitHubReleases { username, repository, .. }) =>
        println!("updates from github.com/{username}/{repository}"),
    Some(other) => println!("transport: {}", other.transport()),
    None => {}
}
# Ok::<(), appimage::Error>(())
```

## Packing

Build an AppImage from an AppDir

```rust,ignore
use appimage::{AppImage, Compression};

let image = AppImage::pack("MyApp.AppDir")
    .runtime(std::fs::read("runtime-x86_64")?)   // the official AppImage runtime, or any ELF
    .compression(Compression::Gzip)
    .write_to("MyApp.AppImage")?;
# Ok::<(), appimage::Error>(())
```

## Mounting (Linux)

```rust,ignore
let image = appimage::AppImage::open("App.AppImage")?;
let mount = image.mount()?;          // spawns a background FUSE session
let apprun = mount.path().join("AppRun");
// use that tree
drop(mount);                         // unmounts and removes the temp mountpoint
# Ok::<(), appimage::Error>(())
```

## License

MIT. See [`LICENSE`](LICENSE) and [`THIRD_PARTY_NOTICES`](THIRD_PARTY_NOTICES).
