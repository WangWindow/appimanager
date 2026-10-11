//! End-to-end tests against a self-contained synthetic AppImage.
//!
//! The fixture (see `common`) is a real SquashFS appended to a hand-crafted
//! ELF, so these tests run everywhere and actually exercise ELF parsing,
//! section reads, and SquashFS extraction — no external image required.

mod common;

use std::fs;

use appimage::{AppImage, Error, Payload, Temp};
use common::{Fixture, APPRUN_CONTENTS, DATA_CONTENTS, NESTED_CONTENTS};

#[test]
fn open_locates_the_squashfs_offset() {
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open synthetic appimage");

    let image_size = fs::metadata(&fixture.path).expect("stat").len();
    assert_eq!(image.offset(), fixture.offset);
    assert!(image.offset() > 0 && image.offset() < image_size);
    assert_eq!(image.path(), fixture.path);
}

#[test]
fn open_rejects_non_elf_input_everywhere() {
    let error = AppImage::open("/dev/null").expect_err("/dev/null is not an ELF");
    assert!(
        matches!(error, Error::InvalidImage { .. } | Error::Io(_)),
        "unexpected error: {error:?}",
    );
    assert!(!error.is_unsupported_platform());
}

#[test]
fn reads_named_elf_sections() {
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open");

    let upd = image
        .update_information()
        .expect("read .upd_info")
        .expect("has .upd_info");
    assert_eq!(upd.to_string_lossy(), fixture.upd_info);

    // Present-but-empty and absent sections both read as None.
    assert_eq!(image.signature().expect("read .sha256_sig"), None);
    assert_eq!(image.section(".missing").expect("read missing"), None);
}

#[test]
fn unpacks_the_full_tree_with_contents_and_symlink() {
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open");

    let out = fixture.dir.path().join("full");
    let unpacked = image.unpack().to(&out).expect("unpack");
    assert_eq!(unpacked.path(), out.as_path());

    assert_eq!(
        fs::read_to_string(unpacked.join("AppRun")).unwrap(),
        APPRUN_CONTENTS
    );
    assert!(unpacked.has_app_run());
    assert_eq!(
        fs::read_to_string(unpacked.join("data")).unwrap(),
        DATA_CONTENTS
    );
    assert_eq!(
        fs::read_to_string(unpacked.join("dir/nested")).unwrap(),
        NESTED_CONTENTS
    );

    let link = fs::read_link(unpacked.join("symlink")).expect("symlink is a symlink");
    assert_eq!(link, std::path::Path::new("data"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(unpacked.join("AppRun"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "executable bit preserved");
    }
}

/// Assert that a fixture's full tree extracts with the expected contents. Shared
/// by the xz and zstd cases so every compressor path is checked the same way.
#[cfg(any(feature = "xz", feature = "zstd"))]
fn assert_tree_extracts(fixture: &Fixture) {
    let image = AppImage::open(&fixture.path).expect("open");
    let out = fixture.dir.path().join("tree");
    let unpacked = image.unpack().to(&out).expect("unpack");

    assert_eq!(
        fs::read_to_string(unpacked.join("AppRun")).unwrap(),
        APPRUN_CONTENTS
    );
    assert_eq!(
        fs::read_to_string(unpacked.join("data")).unwrap(),
        DATA_CONTENTS
    );
    assert_eq!(
        fs::read_to_string(unpacked.join("dir/nested")).unwrap(),
        NESTED_CONTENTS
    );
    assert_eq!(
        fs::read_link(unpacked.join("symlink")).unwrap(),
        std::path::Path::new("data")
    );
}

// The payloads below are real SquashFS images compressed with xz / zstd; the
// crate must decode them with its pure-Rust decoders (no C backend enabled).

#[cfg(feature = "zstd")]
#[test]
fn unpacks_zstd_compressed_payload_pure_rust() {
    let fixture = Fixture::wrap(include_bytes!("fixtures/tree-zstd.sqfs"));
    assert_tree_extracts(&fixture);
}

#[cfg(feature = "xz")]
#[test]
fn unpacks_xz_compressed_payload_pure_rust() {
    let fixture = Fixture::wrap(include_bytes!("fixtures/tree-xz.sqfs"));
    assert_tree_extracts(&fixture);
}

#[test]
fn matching_pattern_extracts_only_selected_paths() {
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open");

    let out = fixture.dir.path().join("pattern");
    image
        .unpack()
        .matching("data")
        .to(&out)
        .expect("unpack matching");

    assert!(out.join("data").is_file());
    assert!(!out.join("AppRun").exists());
    assert!(!out.join("symlink").exists());
}

#[test]
fn unpack_to_temp_yields_self_cleaning_scratch() {
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open");

    let scratch = image.unpack().to(Temp).expect("unpack to scratch");
    let path = scratch.path().to_path_buf();
    assert!(path.join("data").is_file());
    drop(scratch);
    assert!(!path.exists(), "scratch removed on drop");
}

#[test]
fn parses_typed_update_information() {
    use appimage::UpdateInformation;
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open");
    let info = image.update_info().expect("read").expect("has update info");
    assert_eq!(
        info,
        UpdateInformation::Zsync {
            url: "https://example.test/App-latest.zsync".into()
        }
    );
    assert_eq!(info.transport(), "zsync");
}

#[test]
fn extraction_preserves_mtime() {
    use std::time::{Duration, UNIX_EPOCH};
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open");
    let out = fixture.dir.path().join("mtime");
    let unpacked = image.unpack().to(&out).expect("unpack");

    let modified = fs::metadata(unpacked.join("data"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        modified,
        UNIX_EPOCH + Duration::from_secs(u64::from(common::MTIME))
    );
}

#[test]
fn pack_then_open_and_unpack_roundtrips() {
    let fixture = Fixture::new(); // reused only for its temp dir
    let src = fixture.dir.path().join("AppDir");
    fs::create_dir_all(src.join("dir")).unwrap();
    fs::write(src.join("AppRun"), APPRUN_CONTENTS).unwrap();
    fs::write(src.join("data"), DATA_CONTENTS).unwrap();
    fs::write(src.join("dir/nested"), NESTED_CONTENTS).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("data", src.join("symlink")).unwrap();

    let out = fixture.dir.path().join("Packed.AppImage");
    let image = AppImage::pack(&src)
        .runtime(common::runtime_elf())
        .compression(appimage::Compression::Gzip)
        .write_to(&out)
        .expect("pack");

    assert_eq!(image.offset(), common::runtime_elf().len() as u64);

    let extracted = fixture.dir.path().join("unpacked");
    let unpacked = image.unpack().to(&extracted).expect("unpack packed image");
    assert_eq!(
        fs::read_to_string(unpacked.join("AppRun")).unwrap(),
        APPRUN_CONTENTS
    );
    assert_eq!(
        fs::read_to_string(unpacked.join("data")).unwrap(),
        DATA_CONTENTS
    );
    assert_eq!(
        fs::read_to_string(unpacked.join("dir/nested")).unwrap(),
        NESTED_CONTENTS
    );
    #[cfg(unix)]
    assert_eq!(
        fs::read_link(unpacked.join("symlink")).unwrap(),
        std::path::Path::new("data")
    );
}

#[test]
fn pack_roundtrips_with_every_compression_backend() {
    use appimage::Compression;

    // README documents all four as pure-Rust options for `Packer::compression`;
    // exercise each one end to end (pack -> open -> unpack) rather than just Gzip.
    for compression in [
        Compression::Gzip,
        Compression::Lz4,
        Compression::Lzo,
        Compression::None,
    ] {
        let fixture = Fixture::new(); // reused only for its temp dir
        let src = fixture.dir.path().join("AppDir");
        fs::create_dir_all(src.join("dir")).unwrap();
        fs::write(src.join("AppRun"), APPRUN_CONTENTS).unwrap();
        fs::write(src.join("data"), DATA_CONTENTS).unwrap();
        fs::write(src.join("dir/nested"), NESTED_CONTENTS).unwrap();

        let out = fixture.dir.path().join("Packed.AppImage");
        let image = AppImage::pack(&src)
            .runtime(common::runtime_elf())
            .compression(compression)
            .write_to(&out)
            .unwrap_or_else(|error| panic!("pack with {compression:?} failed: {error}"));

        let extracted = fixture.dir.path().join("unpacked");
        let unpacked = image
            .unpack()
            .to(&extracted)
            .unwrap_or_else(|error| panic!("unpack with {compression:?} failed: {error}"));
        assert_eq!(
            fs::read_to_string(unpacked.join("AppRun")).unwrap(),
            APPRUN_CONTENTS
        );
        assert_eq!(
            fs::read_to_string(unpacked.join("data")).unwrap(),
            DATA_CONTENTS
        );
        assert_eq!(
            fs::read_to_string(unpacked.join("dir/nested")).unwrap(),
            NESTED_CONTENTS,
            "compression {compression:?} round-trip mismatch",
        );
    }
}

#[test]
fn pack_without_runtime_is_an_error() {
    let fixture = Fixture::new();
    let src = fixture.dir.path().join("Empty.AppDir");
    fs::create_dir_all(&src).unwrap();
    let err = AppImage::pack(&src)
        .write_to(fixture.dir.path().join("x.AppImage"))
        .expect_err("missing runtime must fail");
    assert!(matches!(err, Error::Pack(_)), "got {err:?}");
}

#[test]
fn sidecars_are_adjacent_paths() {
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open");

    assert!(image.home().path().ends_with("App.AppImage.home"));
    assert!(image.config().path().ends_with("App.AppImage.config"));
    assert!(!image.home().exists());

    let home = image.home().ensure().expect("create sidecar");
    assert!(home.path().is_dir());
    assert!(image.home().exists());
}

#[cfg(target_os = "linux")]
#[test]
fn mount_serves_the_payload_when_fuse_is_available() {
    let fixture = Fixture::new();
    let image = AppImage::open(&fixture.path).expect("open");

    // FUSE may be unavailable (no /dev/fuse, no fusermount3) in CI sandboxes;
    // a mount failure is tolerated, but a successful mount must serve files.
    match image.mount() {
        Ok(mount) => {
            let mountpoint = mount.path().to_path_buf();
            assert!(mountpoint.is_dir());
            assert_eq!(
                fs::read_to_string(mountpoint.join("data")).unwrap(),
                DATA_CONTENTS
            );
            assert_eq!(
                fs::read_to_string(mountpoint.join("dir/nested")).unwrap(),
                NESTED_CONTENTS
            );
            assert_eq!(
                fs::read_link(mountpoint.join("symlink")).unwrap(),
                std::path::Path::new("data")
            );
            drop(mount);
            assert!(!mountpoint.exists(), "owned mountpoint removed on unmount");
        }
        Err(error) => {
            eprintln!("skipping mount assertions: {error}");
        }
    }
}
