//! End-to-end smoke tests for the real `aria-memo` binary.
//!
//! These run the compiled executable (not just `entry()`), which is the only
//! way to execute the process entry point `fn main` — including its
//! `std::process::exit` behaviour. All commands are offline and use an
//! in-memory database, so no state leaks between runs.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aria-memo"))
}

#[test]
fn version_exits_zero() {
    // `--version` prints the bare version number (clap's Version action).
    let out = bin().arg("--version").output().expect("run aria-memo");
    assert!(out.status.success(), "status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.chars().any(|c| c.is_ascii_digit()),
        "stdout: {stdout}"
    );

    // The explicit `version` subcommand prints the product name.
    let out = bin().arg("version").output().expect("run aria-memo");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("aria-memo"), "stdout: {stdout}");
}

#[test]
fn runtime_error_exits_one_with_a_message_on_stderr() {
    let out = bin()
        .args([
            "--db",
            ":memory:",
            "update",
            "--id",
            "missing",
            "--content",
            "x",
        ])
        .output()
        .expect("run aria-memo");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error:"), "stderr: {stderr}");
}

#[test]
fn usage_error_exits_two() {
    let out = bin()
        .arg("--definitely-not-a-flag")
        .output()
        .expect("run aria-memo");
    assert_eq!(out.status.code(), Some(2));
}
