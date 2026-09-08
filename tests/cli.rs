use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

const PASSWORD_ENV: &str = "RGX_CLI_TEST_PASSWORD";
const PASSWORD: &str = "correct horse battery staple for rgx tests";

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rgx"))
}

fn run(args: &[&str], home: Option<&Path>, password: bool) -> Output {
    let mut command = Command::new(binary());
    command.args(args);
    if let Some(home) = home {
        command.env("HOME", home);
        command.env("USERPROFILE", home);
        command.env("APPDATA", home.join("AppData/Roaming"));
    }
    if password {
        command.env(PASSWORD_ENV, PASSWORD);
    }
    command.output().expect("failed to launch rgx")
}

fn ok(args: &[&str], home: Option<&Path>, password: bool) -> Output {
    let output = run(args, home, password);
    assert!(
        output.status.success(),
        "rgx {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn path_arg(path: &Path) -> &str {
    path.to_str().expect("test path must be UTF-8")
}

fn make_source(root: &Path) -> PathBuf {
    let source = root.join("source");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("hello.txt"), b"hello from rgx\n").unwrap();
    fs::write(
        source.join("nested/note.txt"),
        b"recipient and private mode\n",
    )
    .unwrap();
    let repeated = vec![0x5Au8; 512 * 1024];
    fs::write(source.join("duplicate-a.bin"), &repeated).unwrap();
    fs::write(source.join("nested/duplicate-b.bin"), &repeated).unwrap();
    source
}

#[test]
fn cli_plain_private_and_recipient_roundtrip() {
    let temp = tempdir().unwrap();
    let source = make_source(temp.path());

    let plain = temp.path().join("plain.rgx");
    ok(&["pack", path_arg(&source), path_arg(&plain)], None, false);
    ok(&["verify", path_arg(&plain)], None, false);
    let list = ok(&["list", path_arg(&plain)], None, false);
    assert!(String::from_utf8_lossy(&list.stdout).contains("source/nested/note.txt"));
    let info = ok(&["info", path_arg(&plain)], None, false);
    assert!(String::from_utf8_lossy(&info.stdout).contains("Deduplicated logical bytes"));
    let found = ok(&["find", path_arg(&plain), "note"], None, false);
    assert!(String::from_utf8_lossy(&found.stdout).contains("source/nested/note.txt"));
    let cat = ok(
        &["cat", path_arg(&plain), "source/nested/note.txt"],
        None,
        false,
    );
    assert_eq!(cat.stdout, b"recipient and private mode\n");
    let plain_out = temp.path().join("plain-out");
    ok(
        &["extract", path_arg(&plain), path_arg(&plain_out)],
        None,
        false,
    );
    assert_eq!(
        fs::read(plain_out.join("source/nested/note.txt")).unwrap(),
        b"recipient and private mode\n"
    );

    let private = temp.path().join("private.rgx");
    ok(
        &[
            "pack",
            path_arg(&source),
            path_arg(&private),
            "--private",
            "--password-env",
            PASSWORD_ENV,
        ],
        None,
        true,
    );
    ok(
        &["verify", path_arg(&private), "--password-env", PASSWORD_ENV],
        None,
        true,
    );
    let private_cat = ok(
        &[
            "cat",
            path_arg(&private),
            "source/hello.txt",
            "--password-env",
            PASSWORD_ENV,
        ],
        None,
        true,
    );
    assert_eq!(private_cat.stdout, b"hello from rgx\n");
    let private_out = temp.path().join("private-out");
    ok(
        &[
            "extract",
            path_arg(&private),
            path_arg(&private_out),
            "--password-env",
            PASSWORD_ENV,
        ],
        None,
        true,
    );
    assert_eq!(
        fs::read(private_out.join("source/hello.txt")).unwrap(),
        b"hello from rgx\n"
    );

    let identity = temp.path().join("alice_id_rgx");
    ok(&["keygen", "--output", path_arg(&identity)], None, false);
    let public = temp.path().join("alice_id_rgx.pub");
    assert!(identity.is_file());
    assert!(public.is_file());

    let recipient = temp.path().join("recipient.rgx");
    ok(
        &[
            "pack",
            path_arg(&source),
            path_arg(&recipient),
            "--recipient",
            path_arg(&public),
        ],
        None,
        false,
    );
    let verified = ok(
        &[
            "verify",
            path_arg(&recipient),
            "--identity",
            path_arg(&identity),
        ],
        None,
        false,
    );
    assert!(String::from_utf8_lossy(&verified.stdout).contains("Unlocked with RGX identity"));
    let recipient_info = ok(
        &[
            "info",
            path_arg(&recipient),
            "--identity",
            path_arg(&identity),
        ],
        None,
        false,
    );
    assert!(String::from_utf8_lossy(&recipient_info.stdout).contains("Recipient envelope: v2"));
    let recipient_cat = ok(
        &[
            "cat",
            path_arg(&recipient),
            "source/nested/note.txt",
            "--identity",
            path_arg(&identity),
        ],
        None,
        false,
    );
    assert_eq!(recipient_cat.stdout, b"recipient and private mode\n");

    let selected = temp.path().join("recipient-selected");
    ok(
        &[
            "extract",
            path_arg(&recipient),
            path_arg(&selected),
            "--path",
            "source/nested",
            "--identity",
            path_arg(&identity),
        ],
        None,
        false,
    );
    assert!(selected.join("source/nested/note.txt").is_file());
    assert!(!selected.join("source/hello.txt").exists());
}

#[test]
fn cli_auto_identity_password_fallback_and_tamper_rejection() {
    let temp = tempdir().unwrap();
    let source = make_source(temp.path());

    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    ok(&["keygen"], Some(&home), false);
    let auto_private = home.join(".ssh/id_rgx");
    let auto_public = home.join(".ssh/id_rgx.pub");
    assert!(auto_private.is_file());
    assert!(auto_public.is_file());

    let automatic = temp.path().join("automatic.rgx");
    ok(
        &[
            "pack",
            path_arg(&source),
            path_arg(&automatic),
            "--recipient",
            path_arg(&auto_public),
        ],
        Some(&home),
        false,
    );
    let auto_verify = ok(&["verify", path_arg(&automatic)], Some(&home), false);
    assert!(String::from_utf8_lossy(&auto_verify.stdout).contains("Unlocked with RGX identity"));

    let fallback_identity = temp.path().join("fallback_id_rgx");
    ok(
        &["keygen", "--output", path_arg(&fallback_identity)],
        None,
        false,
    );
    let fallback_public = temp.path().join("fallback_id_rgx.pub");
    let fallback_archive = temp.path().join("fallback.rgx");
    ok(
        &[
            "pack",
            path_arg(&source),
            path_arg(&fallback_archive),
            "--recipient",
            path_arg(&fallback_public),
            "--password-fallback",
            "--password-env",
            PASSWORD_ENV,
        ],
        None,
        true,
    );

    let empty_home = temp.path().join("empty-home");
    fs::create_dir_all(&empty_home).unwrap();
    let fallback_verify = ok(
        &[
            "verify",
            path_arg(&fallback_archive),
            "--password-env",
            PASSWORD_ENV,
        ],
        Some(&empty_home),
        true,
    );
    assert!(String::from_utf8_lossy(&fallback_verify.stdout)
        .contains("Unlocked with password fallback"));

    let wrong_password = run(
        &[
            "verify",
            path_arg(&fallback_archive),
            "--password-env",
            PASSWORD_ENV,
        ],
        Some(&empty_home),
        false,
    );
    assert!(!wrong_password.status.success());

    let invalid_combo = run(
        &[
            "pack",
            path_arg(&source),
            path_arg(&temp.path().join("invalid.rgx")),
            "--private",
            "--recipient",
            path_arg(&fallback_public),
        ],
        None,
        false,
    );
    assert!(!invalid_combo.status.success());

    let mut bytes = fs::read(&automatic).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x80;
    let tampered = temp.path().join("tampered.rgx");
    fs::write(&tampered, bytes).unwrap();
    let tamper_verify = run(
        &[
            "verify",
            path_arg(&tampered),
            "--identity",
            path_arg(&auto_private),
        ],
        Some(&home),
        false,
    );
    assert!(!tamper_verify.status.success());
}
