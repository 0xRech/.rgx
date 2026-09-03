use rgx::archive;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::tempdir;

fn deterministic_bytes(len: usize) -> Vec<u8> {
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push((state >> 24) as u8);
    }
    out
}

#[test]
fn directory_roundtrip_preserves_file_contents_and_deduplicates() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let nested = source.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir(source.join("empty")).unwrap();

    let shared = deterministic_bytes(2 * 1024 * 1024);
    fs::write(source.join("alpha.bin"), &shared).unwrap();
    fs::write(nested.join("alpha-copy.bin"), &shared).unwrap();
    fs::write(source.join("hello.txt"), b"hello RGX\nhello RGX\nhello RGX\n").unwrap();

    let archive_path = temp.path().join("test.rgx");
    let packed = archive::pack(&source, &archive_path, 3).unwrap();
    assert_eq!(packed.files, 3);
    assert!(packed.unique_chunks > 0);
    assert!(packed.chunk_references > packed.unique_chunks);
    assert!(packed.deduplicated_bytes >= shared.len() as u64);

    let verified = archive::verify(&archive_path).unwrap();
    assert_eq!(verified.files, 3);
    assert_eq!(verified.deduplicated_bytes, packed.deduplicated_bytes);

    let output = temp.path().join("output");
    archive::extract(&archive_path, &output).unwrap();

    assert_eq!(
        fs::read(source.join("alpha.bin")).unwrap(),
        fs::read(output.join("source/alpha.bin")).unwrap()
    );
    assert_eq!(
        fs::read(nested.join("alpha-copy.bin")).unwrap(),
        fs::read(output.join("source/nested/alpha-copy.bin")).unwrap()
    );
    assert_eq!(
        fs::read(source.join("hello.txt")).unwrap(),
        fs::read(output.join("source/hello.txt")).unwrap()
    );
    assert!(output.join("source/empty").is_dir());
}

#[test]
fn content_defined_chunking_recovers_reuse_after_inserted_prefix() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(&source).unwrap();

    let base = deterministic_bytes(4 * 1024 * 1024);
    fs::write(source.join("base.bin"), &base).unwrap();

    let mut shifted = vec![0x5au8; 32 * 1024];
    shifted.extend_from_slice(&base);
    fs::write(source.join("shifted.bin"), &shifted).unwrap();

    let archive_path = temp.path().join("shifted.rgx");
    let info = archive::pack(&source, &archive_path, 3).unwrap();

    assert!(info.deduplicated_bytes > 2 * 1024 * 1024);
    archive::verify(&archive_path).unwrap();
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

#[test]
fn packing_rejects_output_inside_source_directory() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("data.txt"), b"data").unwrap();

    let archive_path = source.join("bad.rgx");
    assert!(archive::pack(&source, &archive_path, 3).is_err());
    assert!(!archive_path.exists());
}

#[test]
fn verify_detects_corrupted_chunk_payload() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("payload.bin");
    fs::write(&source, deterministic_bytes(512 * 1024)).unwrap();
    let archive_path = temp.path().join("payload.rgx");
    archive::pack(&source, &archive_path, 3).unwrap();

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&archive_path)
        .unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    let marker = bytes
        .windows(4)
        .position(|window| window == b"CHNK")
        .expect("archive should contain a chunk");
    let payload_offset = marker + 56;
    assert!(payload_offset < bytes.len());

    file.seek(SeekFrom::Start(payload_offset as u64)).unwrap();
    let original = bytes[payload_offset];
    file.write_all(&[original ^ 0x01]).unwrap();
    file.flush().unwrap();

    assert!(archive::verify(&archive_path).is_err());
}
