use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use openkakao_cli::bujamentor_service::{
    observe_health, NotificationOutcome, Notifier, HEALTH_INTERVAL_SECS,
};

#[derive(Debug, Parser)]
#[command(name = "openkakao-bujamentor-health")]
struct Cli {
    #[arg(long)]
    status_path: PathBuf,
    #[arg(long)]
    alerts_path: PathBuf,
    #[arg(long)]
    log_path: PathBuf,
    #[arg(long, default_value_t = HEALTH_INTERVAL_SECS)]
    interval_secs: u64,
}

struct OsascriptNotifier;

impl Notifier for OsascriptNotifier {
    fn notify(&mut self, class: &str, reason: &str) -> NotificationOutcome {
        let title = "OpenKakao Bujamentor";
        let message = match class {
            "recovered" => format!("watch recovered from {reason}"),
            other => format!("watch {other}: {reason}"),
        };
        match Command::new("/usr/bin/osascript")
            .env_clear()
            .env("CLASS", class)
            .env("REASON", reason)
            .arg("-e")
            .arg("on run argv")
            .arg("-e")
            .arg("display notification (item 2 of argv) with title (item 1 of argv)")
            .arg("-e")
            .arg("end run")
            .arg(title)
            .arg(message)
            .status()
        {
            Ok(status) if status.success() => NotificationOutcome::Submitted,
            Ok(_) => NotificationOutcome::Rejected,
            Err(_) => NotificationOutcome::Unknown,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut notifier = OsascriptNotifier;
    loop {
        observe_health(
            Utc::now(),
            &cli.status_path,
            &cli.alerts_path,
            &cli.log_path,
            &mut notifier,
        )?;
        thread::sleep(Duration::from_secs(cli.interval_secs));
    }
}
