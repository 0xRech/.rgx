use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::tempdir;

const PASSWORD_ENV: &str = "RGX_RELATIVE_TEST_PASSWORD";
const PASSWORD: &str = "rgx relative output regression password";

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
fn relative_archive_outputs_work_for_all_protection_modes() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("source/nested")).unwrap();
    fs::write(root.join("source/file.txt"), b"relative output\n").unwrap();
    fs::write(root.join("source/nested/file.txt"), b"nested relative output\n").unwrap();

    run(Command::new(binary()).current_dir(root).args([
        "pack",
        "source",
        "plain.rgx",
    ]));
    run(Command::new(binary()).current_dir(root).args([
        "verify",
        "plain.rgx",
    ]));

    run(
        Command::new(binary())
            .current_dir(root)
            .env(PASSWORD_ENV, PASSWORD)
            .args([
                "pack",
                "source",
                "private.rgx",
                "--private",
                "--password-env",
                PASSWORD_ENV,
            ]),
    );
    run(
        Command::new(binary())
            .current_dir(root)
            .env(PASSWORD_ENV, PASSWORD)
            .args(["verify", "private.rgx", "--password-env", PASSWORD_ENV]),
    );

    run(Command::new(binary()).current_dir(root).args([
        "keygen",
        "--output",
        "id_rgx",
    ]));
    run(Command::new(binary()).current_dir(root).args([
        "pack",
        "source",
        "recipient.rgx",
        "--recipient",
        "id_rgx.pub",
    ]));
    run(Command::new(binary()).current_dir(root).args([
        "verify",
        "recipient.rgx",
        "--identity",
        "id_rgx",
    ]));

    assert!(root.join("plain.rgx").is_file());
    assert!(root.join("private.rgx").is_file());
    assert!(root.join("recipient.rgx").is_file());
}
