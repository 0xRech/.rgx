use rgx::recipient::{self, RgxPrivateKey};
use rgx::recipient_archive;
use std::fs;
use tempfile::tempdir;

#[test]
fn recipient_archive_roundtrip_uses_only_matching_identity() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("message.txt"), b"recipient protected payload").unwrap();

    let alice_private = temp.path().join("alice_id_rgx");
    let alice = RgxPrivateKey::generate();
    let alice_public_path = recipient::save_keypair(&alice_private, &alice).unwrap();
    let alice_public = recipient::load_public_key(&alice_public_path).unwrap();

    let bob_private = temp.path().join("bob_id_rgx");
    let bob = RgxPrivateKey::generate();
    recipient::save_keypair(&bob_private, &bob).unwrap();

    let archive = temp.path().join("recipient.rgx");
    recipient_archive::pack_recipient(&source, &archive, 3, &[alice_public], None).unwrap();

    assert!(recipient_archive::try_unlock_identity(&archive, Some(&bob_private)).is_err());
    let (archive_key, _) = recipient_archive::try_unlock_identity(&archive, Some(&alice_private))
        .unwrap()
        .unwrap();

    recipient_archive::verify(&archive, &archive_key).unwrap();
    let output = temp.path().join("output");
    recipient_archive::extract(&archive, &output, None, &archive_key).unwrap();
    assert_eq!(
        fs::read(output.join("source/message.txt")).unwrap(),
        b"recipient protected payload"
    );
}

#[test]
fn multiple_recipients_receive_the_same_archive_key() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("payload.txt");
    fs::write(&source, b"shared recipient payload").unwrap();

    let first_private = temp.path().join("first_id_rgx");
    let first = RgxPrivateKey::generate();
    let first_public_path = recipient::save_keypair(&first_private, &first).unwrap();
    let first_public = recipient::load_public_key(&first_public_path).unwrap();

    let second_private = temp.path().join("second_id_rgx");
    let second = RgxPrivateKey::generate();
    let second_public_path = recipient::save_keypair(&second_private, &second).unwrap();
    let second_public = recipient::load_public_key(&second_public_path).unwrap();

    let archive = temp.path().join("shared.rgx");
    recipient_archive::pack_recipient(
        &source,
        &archive,
        3,
        &[first_public, second_public],
        Some("emergency fallback password"),
    )
    .unwrap();

    let (first_key, _) = recipient_archive::try_unlock_identity(&archive, Some(&first_private))
        .unwrap()
        .unwrap();
    let (second_key, _) = recipient_archive::try_unlock_identity(&archive, Some(&second_private))
        .unwrap()
        .unwrap();
    let (password_key, _) =
        recipient_archive::unlock_password(&archive, "emergency fallback password").unwrap();

    assert_eq!(first_key.as_ref(), second_key.as_ref());
    assert_eq!(first_key.as_ref(), password_key.as_ref());
    assert!(recipient_archive::unlock_password(&archive, "wrong password").is_err());
}
