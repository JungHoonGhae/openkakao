use openkakao_rs::error::OpenKakaoError;

#[test]
fn token_expired_error_is_retryable() {
    assert!(OpenKakaoError::TokenExpired.is_retryable());
}

#[test]
fn loco_status_minus_300_is_retryable() {
    let err = OpenKakaoError::loco("TEST", -300);
    assert!(err.is_retryable());
}

#[test]
fn loco_status_minus_500_is_retryable() {
    let err = OpenKakaoError::loco("TEST", -500);
    assert!(err.is_retryable());
}

#[test]
fn loco_status_minus_400_is_not_retryable() {
    let err = OpenKakaoError::loco("TEST", -400);
    assert!(!err.is_retryable());
}

#[test]
fn loco_minus_950_becomes_token_expired() {
    let err = OpenKakaoError::loco("TEST", -950);
    assert!(matches!(err, OpenKakaoError::TokenExpired));
}

#[test]
fn loco_with_body_preserves_body() {
    let body = bson::doc! { "key": "value" };
    let err = OpenKakaoError::loco_with_body("TEST", -400, body.clone());
    if let OpenKakaoError::LocoStatus { body: Some(b), .. } = err {
        assert_eq!(b.get_str("key").unwrap(), "value");
    } else {
        panic!("Expected LocoStatus with body");
    }
}

#[test]
fn loco_with_body_minus_950_becomes_token_expired() {
    let body = bson::doc! { "key": "value" };
    let err = OpenKakaoError::loco_with_body("TEST", -950, body);
    assert!(matches!(err, OpenKakaoError::TokenExpired));
}

#[test]
fn network_transient_error_is_retryable() {
    let err = OpenKakaoError::Network {
        message: "timeout".to_string(),
        is_transient: true,
    };
    assert!(err.is_retryable());
}

#[test]
fn network_permanent_error_is_not_retryable() {
    let err = OpenKakaoError::Network {
        message: "DNS failed".to_string(),
        is_transient: false,
    };
    assert!(!err.is_retryable());
}

#[test]
fn safety_block_not_retryable() {
    let err = OpenKakaoError::SafetyBlock("test".to_string());
    assert!(!err.is_retryable());
}

#[test]
fn rest_api_error_not_retryable() {
    let err = OpenKakaoError::RestApi {
        status: 401,
        message: "unauthorized".to_string(),
    };
    assert!(!err.is_retryable());
}

#[test]
fn io_error_connection_reset_is_transient() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
    let err: OpenKakaoError = io_err.into();
    assert!(err.is_retryable());
}

#[test]
fn io_error_connection_aborted_is_transient() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "aborted");
    let err: OpenKakaoError = io_err.into();
    assert!(err.is_retryable());
}

#[test]
fn io_error_broken_pipe_is_transient() {
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken pipe");
    let err: OpenKakaoError = io_err.into();
    assert!(err.is_retryable());
}

#[test]
fn io_error_timed_out_is_transient() {
    let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
    let err: OpenKakaoError = io_err.into();
    assert!(err.is_retryable());
}

#[test]
fn io_error_not_found_is_not_transient() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
    let err: OpenKakaoError = io_err.into();
    assert!(!err.is_retryable());
}

#[test]
fn io_error_permission_denied_is_not_transient() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let err: OpenKakaoError = io_err.into();
    assert!(!err.is_retryable());
}

#[test]
fn loco_status_zero_is_not_retryable() {
    let err = OpenKakaoError::loco("TEST", 0);
    assert!(!err.is_retryable());
}

#[test]
fn loco_status_preserves_command_name() {
    let err = OpenKakaoError::loco("LOGINLIST", -300);
    if let OpenKakaoError::LocoStatus { command, status, .. } = err {
        assert_eq!(command, "LOGINLIST");
        assert_eq!(status, -300);
    } else {
        panic!("Expected LocoStatus");
    }
}

#[test]
fn loco_with_body_preserves_command_and_status() {
    let body = bson::doc! { "chatId": 123_i64 };
    let err = OpenKakaoError::loco_with_body("SYNCMSG", -203, body);
    if let OpenKakaoError::LocoStatus {
        command,
        status,
        body: Some(b),
    } = err
    {
        assert_eq!(command, "SYNCMSG");
        assert_eq!(status, -203);
        assert_eq!(b.get_i64("chatId").unwrap(), 123);
    } else {
        panic!("Expected LocoStatus with body");
    }
}

#[test]
fn error_display_includes_command_and_status() {
    let err = OpenKakaoError::loco("CHECKIN", -300);
    let msg = err.to_string();
    assert!(msg.contains("CHECKIN"), "display should include command");
    assert!(msg.contains("-300"), "display should include status");
}

#[test]
fn token_expired_display() {
    let err = OpenKakaoError::TokenExpired;
    let msg = err.to_string();
    assert!(msg.contains("-950") || msg.contains("expired"), "display: {msg}");
}

#[test]
fn safety_block_display_includes_reason() {
    let err = OpenKakaoError::SafetyBlock("open chat detected".to_string());
    let msg = err.to_string();
    assert!(msg.contains("open chat detected"), "display: {msg}");
}
