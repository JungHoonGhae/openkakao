use std::path::PathBuf;

use plist::Value;

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("launchd")
        .join(name)
}

#[test]
fn launchagent_runs_the_durable_notification_receiver() {
    let plist_path = example_path("com.openkakao.notif-watch.plist");
    let plist = Value::from_file(&plist_path).expect("parse notif-watch LaunchAgent example");
    let dict = plist.as_dictionary().expect("LaunchAgent root dictionary");

    assert_eq!(
        dict.get("Label").and_then(Value::as_string),
        Some("com.openkakao.notif-watch")
    );
    assert_eq!(
        dict.get("RunAtLoad").and_then(Value::as_boolean),
        Some(true)
    );
    assert_eq!(
        dict.get("KeepAlive").and_then(Value::as_boolean),
        Some(true)
    );

    let arguments = dict
        .get("ProgramArguments")
        .and_then(Value::as_array)
        .expect("ProgramArguments array");
    assert_eq!(
        arguments.first().and_then(Value::as_string),
        Some("/Users/YOUR_USER/.config/openkakao/openkakao-notif-watch-wrapper.sh")
    );

    let wrapper = std::fs::read_to_string(example_path("openkakao-notif-watch-wrapper.sh"))
        .expect("read notif-watch wrapper example");
    assert!(wrapper.contains("notif-watch"));
    assert!(wrapper.contains("--durable"));
    assert!(!wrapper.contains("\n  watch"));
}
