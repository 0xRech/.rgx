use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let repository_root = manifest_dir
        .join("../..")
        .canonicalize()
        .expect("RGX repository root exists");
    let icons_dir = manifest_dir.join("icons");

    fs::create_dir_all(&icons_dir).expect("failed to create Tauri icon directory");

    for (source_name, destination_name) in [
        ("rgx-file-icon.png", "icon.png"),
        ("rgx-file-icon.ico", "icon.ico"),
    ] {
        let source = repository_root.join("packaging/windows").join(source_name);
        let destination = icons_dir.join(destination_name);
        fs::copy(&source, &destination).unwrap_or_else(|error| {
            panic!(
                "failed to copy RGX app icon {} to {}: {error}",
                source.display(),
                destination.display()
            )
        });
        println!("cargo:rerun-if-changed={}", source.display());
    }

    tauri_build::build()
}
