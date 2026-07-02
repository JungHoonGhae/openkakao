//! AX-automation-based message sending.
//!
//! Drives the real KakaoTalk macOS UI via the Accessibility API instead of
//! the LOCO protocol, so it works even though server login (`-100`) and
//! LOCO auth are broken (see README deprecation notice). No network or
//! KakaoTalk-server contact happens anywhere in this module.
//!
//! Ported from the sibling Swift project kakaocli
//! (https://github.com/silver-flight-group/kakaocli, MIT), with one
//! reliability fix borrowed from steipete's Peekaboo
//! (https://github.com/openclaw/Peekaboo): key/click events are posted
//! directly to KakaoTalk's pid via `CGEventPostToPid` instead of first
//! activating the app to the foreground, which avoids the focus-timing
//! race that causes kakaocli's `send` to hang
//! (https://github.com/silver-flight-group/kakaocli/issues/9).

use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes};
use accessibility_sys::kAXPressAction;
use anyhow::{anyhow, Context, Result};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const KAKAOTALK_BUNDLE_ID: &str = "com.kakao.KakaoTalkMac";
const RETURN_KEYCODE: u16 = 36;
const OPEN_CHAT_TIMEOUT: Duration = Duration::from_secs(5);
const VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
const VERIFY_POLL_INTERVAL: Duration = Duration::from_millis(400);

/// Find the running KakaoTalk process id via `pgrep -x`.
///
/// We shell out rather than link `NSRunningApplication`/AppKit bindings
/// because this is the only place we need a pid lookup and it keeps the
/// dependency surface small (matches `local_db.rs`'s existing convention of
/// shelling out to `ioreg` for platform info).
pub fn find_kakaotalk_pid() -> Result<i32> {
    let output = Command::new("pgrep")
        .args(["-x", "KakaoTalk"])
        .output()
        .context("failed to run pgrep")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<i32>().ok())
        .ok_or_else(|| {
            anyhow!("KakaoTalk is not running (or `{KAKAOTALK_BUNDLE_ID}` not found) — open it and log in first")
        })
}

fn children(el: &AXUIElement) -> Result<Vec<AXUIElement>> {
    let arr = el
        .children()
        .map_err(|e| anyhow!("AXChildren read failed: {e:?}"))?;
    Ok(arr.iter().map(|e| e.clone()).collect())
}

fn role(el: &AXUIElement) -> String {
    el.role().map(|s| s.to_string()).unwrap_or_default()
}

/// Read an element's `AXValue` as a string (works for `AXStaticText` and
/// `AXTextArea`; other value types just fail the downcast and are skipped).
fn value_as_string(el: &AXUIElement) -> Option<String> {
    el.value().ok().and_then(|v| v.downcast::<CFString>()).map(|s| s.to_string())
}

/// Read a string attribute by raw name (works for attributes with no typed
/// accessor in the `accessibility` crate, e.g. `AXIdentifier`).
fn attr_as_string(el: &AXUIElement, name: &str) -> Option<String> {
    let attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new(name));
    el.attribute(&attr)
        .ok()
        .and_then(|v| v.downcast::<CFString>())
        .map(|s| s.to_string())
}

/// Find KakaoTalk's main chat-list window, as opposed to any individual
/// open-chat windows (which are separate `AXWindow`s titled with the
/// other party's — or your own, for the self chat — display name).
fn find_main_window(app: &AXUIElement) -> Result<AXUIElement> {
    let windows = app
        .windows()
        .map_err(|e| anyhow!("AXWindows read failed: {e:?}"))?;
    windows
        .iter()
        .find(|w| attr_as_string(w, "AXIdentifier").as_deref() == Some("Main Window"))
        .map(|w| w.clone())
        .ok_or_else(|| anyhow!("could not find KakaoTalk's main chat-list window — is it open?"))
}

fn find_descendants_by_role(root: &AXUIElement, target_role: &str, out: &mut Vec<AXUIElement>) {
    if role(root) == target_role {
        out.push(root.clone());
    }
    if let Ok(kids) = children(root) {
        for kid in kids {
            find_descendants_by_role(&kid, target_role, out);
        }
    }
}

/// Post a CGEvent to KakaoTalk's pid directly (no `activate()` foreground
/// switch — this is the Peekaboo-style fix for the focus race that hangs
/// kakaocli's send path).
fn post_key_to_pid(pid: i32, keycode: u16, key_down: bool) -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow!("failed to create CGEventSource"))?;
    let event = CGEvent::new_keyboard_event(source, keycode, key_down)
        .map_err(|_| anyhow!("failed to create keyboard CGEvent"))?;
    event.post_to_pid(pid);
    Ok(())
}

fn press_return(pid: i32) -> Result<()> {
    post_key_to_pid(pid, RETURN_KEYCODE, true)?;
    post_key_to_pid(pid, RETURN_KEYCODE, false)?;
    Ok(())
}

/// Type `text` into the focused field by posting one keyboard CGEvent pair
/// per character directly to KakaoTalk's pid, using the Unicode string
/// payload (`CGEventKeyboardSetUnicodeString`) so Hangul input works without
/// needing per-character keycode mapping.
fn type_text_to_pid(pid: i32, text: &str) -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow!("failed to create CGEventSource"))?;
    for down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), 0, down)
            .map_err(|_| anyhow!("failed to create keyboard CGEvent"))?;
        let utf16: Vec<u16> = text.encode_utf16().collect();
        event.set_string_from_utf16_unchecked(&utf16);
        event.post_to_pid(pid);
    }
    Ok(())
}

/// Switch the main window to the chat-list ("chatrooms") tab if it isn't
/// already there — the chat-list `AXTable` only exists while that tab is
/// active; the Friends tab renders an `AXOutline` instead. Left over from an
/// earlier manual tab switch during development, this makes `open_chat_row`
/// resilient to whatever tab the window happens to be on.
fn ensure_chatrooms_tab(main_window: &AXUIElement) {
    let mut tables = Vec::new();
    find_descendants_by_role(main_window, "AXTable", &mut tables);
    if !tables.is_empty() {
        return;
    }
    let mut buttons = Vec::new();
    find_descendants_by_role(main_window, "AXButton", &mut buttons);
    if let Some(tab) = buttons
        .iter()
        .find(|b| attr_as_string(b, "AXIdentifier").as_deref() == Some("chatrooms"))
    {
        let _ = tab.perform_action(&CFString::new(kAXPressAction));
        sleep(Duration::from_millis(400));
    }
}

fn open_chat_row(app: &AXUIElement, chat_display_name: &str) -> Result<()> {
    let main_window = find_main_window(app)?;
    ensure_chatrooms_tab(&main_window);
    let mut tables = Vec::new();
    find_descendants_by_role(&main_window, "AXTable", &mut tables);
    let table = tables
        .first()
        .ok_or_else(|| anyhow!("could not find chat list table in KakaoTalk's AX tree"))?;

    let mut rows = Vec::new();
    find_descendants_by_role(table, "AXRow", &mut rows);

    // Match on the row's first AXStaticText (the chat name) exactly, not a
    // substring — e.g. "Alice" must not accidentally match an "Alice & Bob"
    // group chat. If more than one row has the exact same display name,
    // refuse to guess rather than silently picking one (there is no chat-id
    // to disambiguate with — see SafetyConfig::allowed_send_chats).
    let mut matches = Vec::new();
    for row in &rows {
        let mut texts = Vec::new();
        find_descendants_by_role(row, "AXStaticText", &mut texts);
        if texts.first().and_then(value_as_string).as_deref() == Some(chat_display_name) {
            matches.push(row);
        }
    }

    let row = match matches.as_slice() {
        [] => {
            return Err(anyhow!(
                "chat '{chat_display_name}' not found in visible/loaded chat list"
            ))
        }
        [only] => *only,
        _ => {
            return Err(anyhow!(
                "chat name '{chat_display_name}' matches {} chats in the visible list — ambiguous, refusing to guess",
                matches.len()
            ))
        }
    };

    // Select via AX attribute (works even for off-screen rows — this is the
    // fix kakaocli landed for its off-screen-row regression) rather than a
    // coordinate-based double click. `AXSelectedRows` is a settable
    // attribute of the *table*, not the row (has no typed accessor in the
    // `accessibility` crate either way, so it's addressed by raw name).
    let selected_rows_attr: AXAttribute<CFType> =
        AXAttribute::new(&CFString::new("AXSelectedRows"));
    let one_row = CFArray::from_CFTypes(std::slice::from_ref(row));
    table
        .set_attribute(&selected_rows_attr, one_row.as_CFType())
        .map_err(|e| anyhow!("failed to select chat row: {e:?}"))?;

    Ok(())
}

/// Search a single root (a window, or the whole app as a fallback) for the
/// message composer: an `AXScrollArea` that wraps an `AXTextArea` but no
/// `AXTable` (which would make it the message list instead).
fn find_input_field_in(root: &AXUIElement) -> Option<AXUIElement> {
    let mut scroll_areas = Vec::new();
    find_descendants_by_role(root, "AXScrollArea", &mut scroll_areas);

    for area in scroll_areas {
        let mut tables = Vec::new();
        find_descendants_by_role(&area, "AXTable", &mut tables);
        if !tables.is_empty() {
            continue; // this scroll area is the message list, not the composer
        }
        let mut text_areas = Vec::new();
        find_descendants_by_role(&area, "AXTextArea", &mut text_areas);
        if let Some(field) = text_areas.into_iter().next() {
            return Some(field);
        }
    }
    None
}

/// Find the composer field, preferring the chat window whose title matches
/// `chat_display_name` (relevant when more than one chat window is already
/// open) and falling back to a whole-app search otherwise.
fn find_input_field(app: &AXUIElement, chat_display_name: &str) -> Result<AXUIElement> {
    if let Ok(windows) = app.windows() {
        if let Some(window) = windows.iter().find(|w| {
            w.title()
                .map(|t| t.to_string())
                .ok()
                .is_some_and(|t| t.contains(chat_display_name))
        }) {
            if let Some(field) = find_input_field_in(&window) {
                return Ok(field);
            }
        }
    }
    find_input_field_in(app).ok_or_else(|| {
        anyhow!("could not find the message input field — is the chat window open and focused?")
    })
}

/// One message bubble scraped from a chat window's AX message list.
#[derive(Debug, Clone)]
pub struct AxMessage {
    /// The time label's `AXHelp` text (full date, e.g. "2026. 6. 17.") if
    /// present, else its plain displayed value (e.g. "14:32").
    pub time: Option<String>,
    pub text: String,
}

/// Scrape every message bubble currently rendered in a chat window's message
/// list, in on-screen (chronological) order. Rows with no `AXTextArea`
/// (date separators, system notices) are skipped.
fn read_visible_messages(window: &AXUIElement) -> Vec<AxMessage> {
    let mut tables = Vec::new();
    find_descendants_by_role(window, "AXTable", &mut tables);
    let Some(table) = tables.first() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    find_descendants_by_role(table, "AXRow", &mut rows);

    rows.iter()
        .filter_map(|row| {
            let mut text_areas = Vec::new();
            find_descendants_by_role(row, "AXTextArea", &mut text_areas);
            let text = text_areas.first().and_then(value_as_string)?;

            let mut static_texts = Vec::new();
            find_descendants_by_role(row, "AXStaticText", &mut static_texts);
            let time = static_texts
                .first()
                .and_then(|t| attr_as_string(t, "AXHelp").or_else(|| value_as_string(t)));

            Some(AxMessage { time, text })
        })
        .collect()
}

/// Scrape the text of every message bubble currently rendered in a chat
/// window's message list, in on-screen (chronological) order.
fn read_visible_message_texts(window: &AXUIElement) -> Vec<String> {
    read_visible_messages(window).into_iter().map(|m| m.text).collect()
}

/// Find an already-open chat window whose title matches `chat_display_name`
/// (the other party's — or your own, for the self/memo chat — display name).
fn find_chat_window(app: &AXUIElement, chat_display_name: &str) -> Option<AXUIElement> {
    app.windows()
        .ok()?
        .iter()
        .find(|w| {
            w.title()
                .map(|t| t.to_string())
                .ok()
                .is_some_and(|t| t.contains(chat_display_name))
        })
        .map(|w| w.clone())
}

/// Read the most recent `count` messages visible in a chat's AX message list,
/// opening the chat first if it isn't already open. No local SQLCipher DB
/// access, so this works even when `local_db.rs`'s key derivation is stale
/// for the installed KakaoTalk build (see README deprecation notice). Only
/// messages already rendered on screen are returned — older history requires
/// scrolling up in KakaoTalk first.
pub fn read_via_ax(chat_display_name: &str, count: usize) -> Result<Vec<AxMessage>> {
    let pid = find_kakaotalk_pid()?;
    let app = AXUIElement::application(pid);

    open_chat_row(&app, chat_display_name)?;
    press_return(pid)?;

    let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
    let window = loop {
        if let Some(window) = find_chat_window(&app, chat_display_name) {
            if !read_visible_messages(&window).is_empty() {
                break window;
            }
        }
        if Instant::now() >= deadline {
            anyhow::bail!("chat window did not open (or has no visible messages) in time");
        }
        sleep(Duration::from_millis(150));
    };

    let mut messages = read_visible_messages(&window);
    if messages.len() > count {
        messages = messages.split_off(messages.len() - count);
    }
    Ok(messages)
}

/// Send `message` to the chat identified by `chat_display_name` via AX
/// automation, then poll the chat window's own message list (scraped via
/// AX, not the local SQLCipher DB) to confirm delivery instead of trusting
/// a fixed sleep.
///
/// `chat_display_name` should be a substring of the chat's title as shown
/// in the chat list (same matching convention as kakaocli's `send`).
pub fn send_via_ax(chat_display_name: &str, message: &str) -> Result<()> {
    let pid = find_kakaotalk_pid()?;
    let app = AXUIElement::application(pid);

    open_chat_row(&app, chat_display_name)?;
    press_return(pid)?;

    let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
    let field = loop {
        match find_input_field(&app, chat_display_name) {
            Ok(field) => break field,
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(e.context("chat window did not open in time"));
                }
                sleep(Duration::from_millis(150));
            }
        }
    };

    if field.set_value(CFString::new(message).as_CFType()).is_err() {
        type_text_to_pid(pid, message)?;
    }
    press_return(pid)?;

    verify_sent(&app, message, VERIFY_TIMEOUT)
}

/// Poll every open chat window's AX-scraped message list for `message` to
/// appear, instead of a fixed sleep+assume-success. Scanning all windows
/// (rather than matching one by title) sidesteps the fact that a chat
/// window's title is the *other party's* name (or your own name, for the
/// self/"나와의 채팅" memo chat) — not necessarily the string used to select
/// the chat in the list. Needs no extra permission (no Screen Recording,
/// unlike a screenshot-based verification loop) since it reuses the same
/// Accessibility access already granted for sending.
fn verify_sent(app: &AXUIElement, message: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(windows) = app.windows() {
            if windows
                .iter()
                .any(|w| read_visible_message_texts(&w).iter().any(|t| t == message))
            {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "sent the message but could not confirm it appeared in any open chat window within {}s — check KakaoTalk manually",
                timeout.as_secs()
            );
        }
        sleep(VERIFY_POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_chat_timeout_is_bounded_and_shorter_than_verify_timeout() {
        assert!(OPEN_CHAT_TIMEOUT < VERIFY_TIMEOUT);
        assert!(OPEN_CHAT_TIMEOUT.as_secs() > 0);
    }

    #[test]
    fn verify_poll_interval_is_smaller_than_timeout() {
        assert!(VERIFY_POLL_INTERVAL < VERIFY_TIMEOUT);
    }

    #[test]
    fn return_keycode_matches_macos_carbon_constant() {
        // kVK_Return from Carbon HIToolbox/Events.h — used throughout macOS
        // AX/CGEvent automation tools (also what kakaocli's AXHelpers uses).
        assert_eq!(RETURN_KEYCODE, 36);
    }
}
