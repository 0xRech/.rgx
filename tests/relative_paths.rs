use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::tempdir;

const PASSWORD_ENV: &str = "RGX_RELATIVE_TEST_PASSWORD";

fn test_password() -> String {
    format!("rgx-relative-test-{}", std::process::id())
}

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rgx"))
}

fn run(command: &mut Command) -> Output {
    let output = command.output().expect("failed to launch rgx");
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn relative_archive_outputs_and_spaced_paths_work_for_all_protection_modes() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("workspace with spaces & symbols");
    fs::create_dir_all(root.join("source folder/nested folder")).unwrap();
    fs::write(
        root.join("source folder/hello world.txt"),
        b"relative output with spaces\n",
    )
    .unwrap();
    fs::write(
        root.join("source folder/nested folder/file.txt"),
        b"nested relative output\n",
    )
    .unwrap();

    run(Command::new(binary()).current_dir(&root).args([
        "pack",
        "source folder",
        "plain archive.rgx",
    ]));
    run(Command::new(binary())
        .current_dir(&root)
        .args(["verify", "plain archive.rgx"]));
    run(Command::new(binary()).current_dir(&root).args([
        "extract",
        "plain archive.rgx",
        "plain output",
    ]));
    assert_eq!(
        fs::read(root.join("plain output/source folder/hello world.txt")).unwrap(),
        b"relative output with spaces\n"
    );

    run(Command::new(binary()).current_dir(&root).args([
        "extract",
        "plain archive.rgx",
        "selected file output",
        "--path",
        "source folder/hello world.txt",
    ]));
    assert_eq!(
        fs::read(root.join("selected file output/source folder/hello world.txt")).unwrap(),
        b"relative output with spaces\n"
    );
    assert!(!root
        .join("selected file output/source folder/nested folder/file.txt")
        .exists());

    run(Command::new(binary())
        .current_dir(&root)
        .env(PASSWORD_ENV, test_password())
        .args([
            "pack",
            "source folder",
            "private archive.rgx",
            "--private",
            "--password-env",
            PASSWORD_ENV,
        ]));
    run(Command::new(binary())
        .current_dir(&root)
        .env(PASSWORD_ENV, test_password())
        .args([
            "verify",
            "private archive.rgx",
            "--password-env",
            PASSWORD_ENV,
        ]));
    run(Command::new(binary())
        .current_dir(&root)
        .env(PASSWORD_ENV, test_password())
        .args([
            "extract",
            "private archive.rgx",
            "private output",
            "--password-env",
            PASSWORD_ENV,
        ]));
    assert_eq!(
        fs::read(root.join("private output/source folder/nested folder/file.txt")).unwrap(),
        b"nested relative output\n"
    );

    run(Command::new(binary())
        .current_dir(&root)
        .args(["keygen", "--output", "id_rgx"]));
    run(Command::new(binary()).current_dir(&root).args([
        "pack",
        "source folder",
        "recipient archive.rgx",
        "--recipient",
        "id_rgx.pub",
    ]));
    run(Command::new(binary()).current_dir(&root).args([
        "verify",
        "recipient archive.rgx",
        "--identity",
        "id_rgx",
    ]));
    run(Command::new(binary()).current_dir(&root).args([
        "extract",
        "recipient archive.rgx",
        "recipient output",
        "--identity",
        "id_rgx",
    ]));
    assert_eq!(
        fs::read(root.join("recipient output/source folder/hello world.txt")).unwrap(),
        b"relative output with spaces\n"
    );

    assert!(root.join("plain archive.rgx").is_file());
    assert!(root.join("private archive.rgx").is_file());
    assert!(root.join("recipient archive.rgx").is_file());
}
