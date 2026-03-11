use assert_cmd::Command;
use predicates::prelude::*;

fn cmd() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("openkakao-rs").unwrap()
}

#[test]
fn help_exits_zero() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("OpenKakao Rust CLI"));
}

#[test]
fn help_lists_expected_subcommands() {
    let output = cmd().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    for subcmd in &[
        "auth",
        "chats",
        "read",
        "send",
        "watch",
        "doctor",
        "members",
        "delete",
        "mark-read",
    ] {
        assert!(
            stdout.contains(subcmd),
            "--help output should list '{}' subcommand",
            subcmd
        );
    }
}

#[test]
fn version_prints_correct_version() {
    let version = env!("CARGO_PKG_VERSION");
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(version));
}

#[test]
fn invalid_subcommand_exits_nonzero() {
    cmd()
        .arg("nonexistent-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn send_without_args_fails() {
    cmd().arg("send").assert().failure().stderr(
        predicate::str::contains("required arguments").or(predicate::str::contains("Usage")),
    );
}

#[test]
fn read_without_chat_id_fails() {
    cmd().arg("read").assert().failure().stderr(
        predicate::str::contains("required arguments").or(predicate::str::contains("Usage")),
    );
}

#[test]
fn json_flag_is_global() {
    // --json should be accepted before any subcommand
    // doctor doesn't require credentials for basic checks
    cmd().args(["--json", "--help"]).assert().success();
}

#[test]
fn no_color_flag_is_global() {
    cmd().args(["--no-color", "--help"]).assert().success();
}

#[test]
fn watch_accepts_capture_flag() {
    cmd()
        .args(["watch", "--capture", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("capture"));
}

#[test]
fn probe_accepts_capture_pushes_flag() {
    cmd()
        .args(["probe", "PING", "--capture-pushes", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("capture-pushes"));
}

#[test]
fn delete_help_works() {
    cmd()
        .args(["delete", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Delete a message"));
}

#[test]
fn delete_without_args_fails() {
    cmd().arg("delete").assert().failure().stderr(
        predicate::str::contains("required arguments").or(predicate::str::contains("Usage")),
    );
}

#[test]
fn mark_read_help_works() {
    cmd()
        .args(["mark-read", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Mark messages as read"));
}

#[test]
fn mark_read_without_args_fails() {
    cmd().arg("mark-read").assert().failure().stderr(
        predicate::str::contains("required arguments").or(predicate::str::contains("Usage")),
    );
}

#[test]
fn doctor_json_outputs_valid_json() {
    let output = cmd().args(["--json", "doctor"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --json output should be valid JSON");
    assert!(
        parsed.get("checks").is_some(),
        "doctor --json output should have 'checks' key"
    );
    assert!(
        parsed["checks"].is_array(),
        "doctor --json 'checks' should be an array"
    );
}

#[test]
fn auth_status_json_outputs_valid_json() {
    let output = cmd().args(["--json", "auth-status"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("auth-status --json output should be valid JSON");
    assert!(
        parsed.get("consecutive_failures").is_some(),
        "auth-status --json output should have 'consecutive_failures' key"
    );
    assert!(
        parsed.get("path").is_some(),
        "auth-status --json output should have 'path' key"
    );
}

#[test]
fn cache_stats_json_outputs_valid_json() {
    let output = cmd().args(["--json", "cache-stats"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("cache-stats --json output should be valid JSON");
    assert!(
        parsed.get("total_messages").is_some(),
        "cache-stats --json output should have 'total_messages' key"
    );
    assert!(
        parsed.get("chats").is_some(),
        "cache-stats --json output should have 'chats' key"
    );
    assert!(
        parsed["chats"].is_array(),
        "cache-stats --json 'chats' should be an array"
    );
}
