use std::fs;

use assert_cmd::Command;
use openkakao_cli::bujamentor_service::{
    classify_status_path, ClosedReason, HealthClass, WatchStatusRecord,
};

fn cmd() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("openkakao-cli").unwrap()
}

#[test]
fn service_ax_watch_bootstrap_uses_canonical_shared_status_schema() {
    let home = tempfile::tempdir().unwrap();
    let config_dir = home.path().join(".config").join("openkakao");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "this = [ definitely invalid toml",
    )
    .unwrap();

    let state_root = home.path().join("state");
    fs::create_dir_all(&state_root).unwrap();
    let status_path = state_root.join("watch-status.json");
    let log_path = state_root.join("watch.log");

    cmd()
        .env("HOME", home.path())
        .env("OPENKAKAO_CLI_NO_DEPRECATION", "1")
        .args([
            "ax-watch",
            "--service-mode",
            "--interval",
            "5",
            "--status-path",
            status_path.to_str().unwrap(),
            "--log-path",
            log_path.to_str().unwrap(),
        ])
        .assert()
        .failure();

    let persisted: WatchStatusRecord =
        serde_json::from_str(&fs::read_to_string(&status_path).unwrap()).unwrap();
    assert_eq!(persisted.failure, Some(ClosedReason::ConfigInvalid));

    let classified = classify_status_path(chrono::Utc::now(), &status_path);
    assert_eq!(classified.class, HealthClass::Degraded);
    assert_eq!(classified.reason, Some(ClosedReason::ConfigInvalid));
}

#[test]
fn service_ax_watch_rejects_noncanonical_status_path_before_bootstrap_write() {
    let home = tempfile::tempdir().unwrap();
    let config_dir = home.path().join(".config").join("openkakao");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.toml"),
        "this = [ definitely invalid toml",
    )
    .unwrap();

    let state_root = home.path().join("state");
    fs::create_dir_all(&state_root).unwrap();
    let status_path = state_root.join("not-watch-status.json");
    let log_path = state_root.join("watch.log");

    cmd()
        .env("HOME", home.path())
        .env("OPENKAKAO_CLI_NO_DEPRECATION", "1")
        .args([
            "ax-watch",
            "--service-mode",
            "--interval",
            "5",
            "--status-path",
            status_path.to_str().unwrap(),
            "--log-path",
            log_path.to_str().unwrap(),
        ])
        .assert()
        .failure();

    assert!(!status_path.exists());
}
