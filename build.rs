use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let local_po = manifest.join("po");
    let linguas = fs::read_to_string(local_po.join("LINGUAS")).expect("po/LINGUAS is required");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("locale");
    for language in linguas
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let source = local_po.join(format!("{}.po", language));
        assert!(source.exists(), "missing translation: {}", source.display());
        let destination = output.join(language).join("LC_MESSAGES");
        fs::create_dir_all(&destination).expect("create gettext output directory");
        let status = Command::new("msgfmt")
            .arg("--check")
            .arg("-o")
            .arg(destination.join("appimanager.mo"))
            .arg(&source)
            .status()
            .expect("msgfmt is required to build translations");
        if !status.success() {
            panic!("msgfmt failed for {}", source.display());
        }
        println!("cargo:rerun-if-changed={}", source.display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        local_po.join("LINGUAS").display()
    );

    // Keep GSettings functional for an uninstalled `cargo run` as well as an
    // installed application. The schema is built into OUT_DIR rather than
    // writing generated files into the source tree.
    let schema_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("schemas");
    fs::create_dir_all(&schema_dir).expect("create GSettings schema directory");
    let schema = manifest.join("data/io.github.appimanager.gschema.xml");
    fs::copy(
        &schema,
        schema_dir.join("io.github.appimanager.AppImageManager.gschema.xml"),
    )
    .expect("copy GSettings schema");
    let status = Command::new("glib-compile-schemas")
        .arg(&schema_dir)
        .status()
        .expect("glib-compile-schemas is required to build GSettings schemas");
    assert!(status.success(), "glib-compile-schemas failed");
    println!(
        "cargo:rustc-env=APPIMANAGER_SCHEMA_DIR={}",
        schema_dir.display()
    );
    println!("cargo:rerun-if-changed={}", schema.display());
}
