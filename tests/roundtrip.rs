use rgx::archive;
use std::fs;
use tempfile::tempdir;

#[test]
fn directory_roundtrip_preserves_file_contents() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let nested = source.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(source.join("hello.txt"), b"hello RGX\nhello RGX\nhello RGX\n").unwrap();
    fs::write(nested.join("data.bin"), (0u8..=255).collect::<Vec<_>>()).unwrap();
    fs::create_dir(source.join("empty")).unwrap();

    let archive_path = temp.path().join("test.rgx");
    let packed = archive::pack(&source, &archive_path, 3).unwrap();
    assert_eq!(packed.files, 2);

    let verified = archive::verify(&archive_path).unwrap();
    assert_eq!(verified.files, 2);

    let output = temp.path().join("output");
    archive::extract(&archive_path, &output).unwrap();

    assert_eq!(
        fs::read(source.join("hello.txt")).unwrap(),
        fs::read(output.join("source/hello.txt")).unwrap()
    );
    assert_eq!(
        fs::read(nested.join("data.bin")).unwrap(),
        fs::read(output.join("source/nested/data.bin")).unwrap()
    );
    assert!(output.join("source/empty").is_dir());
}

#[test]
fn extraction_rejects_existing_output_directory() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("one.txt");
    fs::write(&source, b"data").unwrap();
    let archive_path = temp.path().join("one.rgx");
    archive::pack(&source, &archive_path, 3).unwrap();

    let output = temp.path().join("output");
    fs::create_dir_all(&output).unwrap();

    assert!(archive::extract(&archive_path, &output).is_err());
}
