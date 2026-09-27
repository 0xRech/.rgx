use image::ImageFormat;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"));
    let repository_root = manifest_dir
        .join("../..")
        .canonicalize()
        .expect("RGX repository root exists");
    let icons_dir = manifest_dir.join("icons");

    fs::create_dir_all(&icons_dir).expect("failed to create Tauri icon directory");

    let ico_source = repository_root
        .join("packaging/windows")
        .join("rgx-file-icon.ico");
    let ico_destination = icons_dir.join("icon.ico");
    fs::copy(&ico_source, &ico_destination).unwrap_or_else(|error| {
        panic!(
            "failed to copy RGX app icon {} to {}: {error}",
            ico_source.display(),
            ico_destination.display()
        )
    });

    let png_destination = icons_dir.join("icon.png");
    let icon = image::open(&ico_source).unwrap_or_else(|error| {
        panic!(
            "failed to decode canonical RGX ICO {}: {error}",
            ico_source.display()
        )
    });
    icon.save_with_format(&png_destination, ImageFormat::Png)
        .unwrap_or_else(|error| {
            panic!(
                "failed to generate Tauri PNG icon {}: {error}",
                png_destination.display()
            )
        });

    println!("cargo:rerun-if-changed={}", ico_source.display());

    tauri_build::build()
}
