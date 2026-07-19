//! Integration tests for CASTERM

use assert_cmd::Command;
use predicates::prelude::*;

/// Test that the binary exists and responds to --version
#[test]
fn test_version_flag() {
    let mut cmd = Command::cargo_bin("casterm").unwrap();
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("casterm"));
}

/// Test that --help works
#[test]
fn test_help_flag() {
    let mut cmd = Command::cargo_bin("casterm").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("terminal"));
}

/// Test that --ui cli mode works
#[test]
fn test_cli_mode() {
    let mut cmd = Command::cargo_bin("casterm").unwrap();
    cmd.args(["--ui", "cli"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("CASTERM"));
}
