use rgx::private::{self, ArchiveKind};
use std::fs;
use std::io::{Seek, SeekFrom, Write};
use tempfile::tempdir;

const PASSWORD: &str = "correct horse battery staple rgx";

#[test]
fn private_archive_roundtrip_hides_plaintext_metadata() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("confidential-project");
    let nested = source.join("internal");
    fs::create_dir_all(&nested).unwrap();

    let repeated = vec![0x41u8; 512 * 1024];
    fs::write(source.join("secret-name.txt"), b"TOP SECRET RGX TEST CONTENT").unwrap();
    fs::write(source.join("copy-a.bin"), &repeated).unwrap();
    fs::write(nested.join("copy-b.bin"), &repeated).unwrap();

    let encrypted = temp.path().join("private.rgx");
    let packed = private::pack_private(&source, &encrypted, 3, PASSWORD).unwrap();
    assert_eq!(private::detect_kind(&encrypted).unwrap(), ArchiveKind::Private);
    assert_eq!(packed.files, 3);
    assert!(packed.deduplicated_bytes >= repeated.len() as u64);

    let encrypted_bytes = fs::read(&encrypted).unwrap();
    assert!(!encrypted_bytes
        .windows(b"secret-name.txt".len())
        .any(|window| window == b"secret-name.txt"));
    assert!(!encrypted_bytes
        .windows(b"TOP SECRET RGX TEST CONTENT".len())
        .any(|window| window == b"TOP SECRET RGX TEST CONTENT"));

    assert!(private::verify_private(&encrypted, "wrong password").is_err());
    private::verify_private(&encrypted, PASSWORD).unwrap();

    let output = temp.path().join("restore");
    private::extract_private(&encrypted, &output, PASSWORD).unwrap();
    assert_eq!(
        fs::read(source.join("secret-name.txt")).unwrap(),
        fs::read(output.join("confidential-project/secret-name.txt")).unwrap()
    );
    assert_eq!(
        fs::read(source.join("copy-a.bin")).unwrap(),
        fs::read(output.join("confidential-project/copy-a.bin")).unwrap()
    );
    assert_eq!(
        fs::read(nested.join("copy-b.bin")).unwrap(),
        fs::read(output.join("confidential-project/internal/copy-b.bin")).unwrap()
    );
}

#[test]
fn private_archive_rejects_ciphertext_tampering() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("payload.txt");
    fs::write(&source, b"authenticated encryption test payload").unwrap();
    let encrypted = temp.path().join("private.rgx");
    private::pack_private(&source, &encrypted, 3, PASSWORD).unwrap();

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&encrypted)
        .unwrap();
    file.seek(SeekFrom::Start(100)).unwrap();
    file.write_all(&[0x7f]).unwrap();
    file.flush().unwrap();

    assert!(private::verify_private(&encrypted, PASSWORD).is_err());
}
