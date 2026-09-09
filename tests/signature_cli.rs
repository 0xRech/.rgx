use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

const KEY_PASSWORD_ENV: &str = "RGX_KEY_TEST_PASSWORD";
const KEY_PASSWORD: &str = "rgx protected key test passphrase";

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rgx"))
}

fn run(command: &mut Command) -> Output {
    command.output().expect("failed to launch rgx")
}

fn ok(command: &mut Command) -> Output {
    let output = run(command);
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn make_source(root: &Path) -> PathBuf {
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("hello.txt"), b"signed RGX test\n").unwrap();
    source
}

#[test]
fn cli_detached_signature_roundtrip_and_tamper_rejection() {
    let temp = tempdir().unwrap();
    let source = make_source(temp.path());
    let archive = temp.path().join("signed.rgx");
    let identity = temp.path().join("signer_id_rgx");
    let public = temp.path().join("signer_id_rgx.pub");
    let signature = temp.path().join("signed.rgx.sig");

    ok(Command::new(binary()).args([
        "pack",
        source.to_str().unwrap(),
        archive.to_str().unwrap(),
    ]));
    ok(Command::new(binary()).args([
        "keygen",
        "--output",
        identity.to_str().unwrap(),
    ]));
    ok(Command::new(binary()).args([
        "sign",
        archive.to_str().unwrap(),
        "--identity",
        identity.to_str().unwrap(),
        "--output",
        signature.to_str().unwrap(),
    ]));
    ok(Command::new(binary()).args([
        "verify-signature",
        archive.to_str().unwrap(),
        signature.to_str().unwrap(),
        "--public-key",
        public.to_str().unwrap(),
    ]));

    let mut bytes = fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x20;
    fs::write(&archive, bytes).unwrap();
    let failed = run(Command::new(binary()).args([
        "verify-signature",
        archive.to_str().unwrap(),
        signature.to_str().unwrap(),
        "--public-key",
        public.to_str().unwrap(),
    ]));
    assert!(!failed.status.success());
}

#[test]
fn cli_protected_identity_can_unlock_recipient_archive() {
    let temp = tempdir().unwrap();
    let source = make_source(temp.path());
    let identity = temp.path().join("protected_id_rgx");
    let public = temp.path().join("protected_id_rgx.pub");
    let archive = temp.path().join("recipient.rgx");

    ok(Command::new(binary())
        .env(KEY_PASSWORD_ENV, KEY_PASSWORD)
        .args([
            "keygen",
            "--output",
            identity.to_str().unwrap(),
            "--protect",
            "--password-env",
            KEY_PASSWORD_ENV,
        ]));
    ok(Command::new(binary()).args([
        "pack",
        source.to_str().unwrap(),
        archive.to_str().unwrap(),
        "--recipient",
        public.to_str().unwrap(),
    ]));
    let verified = ok(Command::new(binary())
        .env("RGX_KEY_PASSWORD", KEY_PASSWORD)
        .args([
            "verify",
            archive.to_str().unwrap(),
            "--identity",
            identity.to_str().unwrap(),
        ]));
    assert!(String::from_utf8_lossy(&verified.stdout).contains("Unlocked with RGX identity"));
}
