//! Minimal consumer: open an AppImage and print its SquashFS offset.
//!
//! Run with a path argument:
//!
//! ```sh
//! cargo run --example static_consumer -- ./Some.AppImage
//! ```

use appimage::AppImage;

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: static_consumer <path-to.AppImage>");
        std::process::exit(2);
    };

    match AppImage::open(&path) {
        Ok(image) => println!("squashfs offset: {}", image.offset()),
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}
