//! OS-mediated user-presence verification for irreversible local actions.

use anyhow::Result;

fn process_validation_error(code: i32) -> Option<&'static str> {
    match code {
        0 => None,
        10 => Some("could not bind a macOS code-signing object to the KakaoTalk process"),
        11 => Some("the running KakaoTalk process failed Apple-anchored dynamic code validation"),
        12 => Some("the running process does not have KakaoTalk's expected bundle/team identity"),
        _ => Some("the running KakaoTalk process failed code-signing validation unexpectedly"),
    }
}

fn authentication_error(code: i32) -> Option<&'static str> {
    match code {
        0 => None,
        1 => Some(
            "macOS device-owner authentication is unavailable; configure Touch ID or a login password before approving sends",
        ),
        2 => Some("macOS device-owner authentication was cancelled or denied; no message was sent"),
        3 => Some("macOS device-owner authentication timed out; no message was sent"),
        _ => Some("macOS device-owner authentication failed unexpectedly; no message was sent"),
    }
}

#[cfg(target_os = "macos")]
pub fn require_device_owner_auth(reason: &str) -> Result<()> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};

    unsafe extern "C" {
        fn openkakao_authenticate_device_owner(reason_utf8: *const c_char) -> c_int;
    }

    let reason = CString::new(reason)?;
    let code = unsafe { openkakao_authenticate_device_owner(reason.as_ptr()) };
    if let Some(message) = authentication_error(code) {
        anyhow::bail!(message);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn validate_kakaotalk_process(pid: i32) -> Result<()> {
    use std::os::raw::c_int;

    unsafe extern "C" {
        fn openkakao_validate_kakaotalk_process(pid: c_int) -> c_int;
    }

    let code = unsafe { openkakao_validate_kakaotalk_process(pid) };
    if let Some(message) = process_validation_error(code) {
        anyhow::bail!(message);
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn require_device_owner_auth(_reason: &str) -> Result<()> {
    anyhow::bail!("safe-send approval with device-owner authentication is only supported on macOS")
}

#[cfg(not(target_os = "macos"))]
pub fn validate_kakaotalk_process(_pid: i32) -> Result<()> {
    anyhow::bail!("KakaoTalk process validation is only supported on macOS")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_authentication_codes_fail_closed() {
        assert_eq!(authentication_error(0), None);
        assert!(authentication_error(1).unwrap().contains("unavailable"));
        assert!(authentication_error(2).unwrap().contains("denied"));
        assert!(authentication_error(3).unwrap().contains("timed out"));
        assert!(authentication_error(99).unwrap().contains("unexpectedly"));
    }

    #[test]
    fn process_validation_codes_fail_closed() {
        assert_eq!(process_validation_error(0), None);
        assert!(process_validation_error(10).unwrap().contains("bind"));
        assert!(process_validation_error(11)
            .unwrap()
            .contains("Apple-anchored"));
        assert!(process_validation_error(12)
            .unwrap()
            .contains("bundle/team"));
        assert!(process_validation_error(99)
            .unwrap()
            .contains("unexpectedly"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn process_validation_rejects_this_non_kakao_test_process() {
        assert!(validate_kakaotalk_process(std::process::id() as i32).is_err());
    }
}
