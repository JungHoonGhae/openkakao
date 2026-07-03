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
//!
//! The real implementation only compiles on macOS — `accessibility`/
//! `core-graphics` link Apple-only frameworks, which fails to even build on
//! other platforms (see `Cargo.toml`'s macOS-only target dependencies). A
//! stub with the same public API stands in on other platforms so the crate
//! still builds and lints in cross-platform CI.

// Only `imp::open_chat_row` (macOS-only) actually calls these outside of
// tests, so on other platforms — where `mod imp` doesn't compile and
// `mod stub` never needs to match a chat row at all — they're otherwise
// flagged as dead code by the real (non-test) build.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ChatMatch {
    Found(usize),
    NotFound,
    Ambiguous(usize),
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn match_chat_row(row_names: &[Option<String>], target: &str) -> ChatMatch {
    let mut matches = row_names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.as_deref() == Some(target));

    match (matches.next(), matches.next()) {
        (None, _) => ChatMatch::NotFound,
        (Some((idx, _)), None) => ChatMatch::Found(idx),
        (Some(_), Some(_)) => {
            let count = row_names
                .iter()
                .filter(|name| name.as_deref() == Some(target))
                .count();
            ChatMatch::Ambiguous(count)
        }
    }
}

#[cfg(test)]
mod match_tests {
    use super::*;

    #[test]
    fn empty_list_is_not_found() {
        assert_eq!(match_chat_row(&[], "Alice"), ChatMatch::NotFound);
    }

    #[test]
    fn single_exact_match_is_found() {
        let names = [Some("Alice".to_string())];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::Found(0));
    }

    #[test]
    fn substring_does_not_match() {
        // "Alice" must not match a group chat named "Alice & Bob".
        let names = [Some("Alice & Bob".to_string())];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::NotFound);
    }

    #[test]
    fn exact_match_among_non_matching_rows() {
        let names = [
            Some("Alice & Bob".to_string()),
            Some("Alice".to_string()),
            Some("Carol".to_string()),
        ];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::Found(1));
    }

    #[test]
    fn duplicate_names_are_ambiguous() {
        let names = [Some("Alice".to_string()), Some("Alice".to_string())];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::Ambiguous(2));
    }

    #[test]
    fn unreadable_rows_are_ignored_not_matched() {
        // A row whose name AX couldn't read (None) must never match, and
        // must not affect matching of the other rows.
        let names = [None, Some("Alice".to_string()), None];
        assert_eq!(match_chat_row(&names, "Alice"), ChatMatch::Found(1));
    }
}

#[cfg(target_os = "macos")]
mod imp {

    use std::process::Command;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes};
    use accessibility_sys::kAXPressAction;
    use accessibility_sys::AXIsProcessTrusted;
    use accessibility_sys::{AXUIElementCopyMultipleAttributeValues, AXUIElementRef};
    use anyhow::{anyhow, Context, Result};
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
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

    /// Check the calling process has been granted Accessibility permission
    /// before touching the AX tree at all. Without this, every AXUIElement
    /// call below just silently fails or returns empty results, which
    /// previously surfaced as a confusing "chat not found" error with no
    /// hint that the real cause was a missing permission grant.
    fn ensure_ax_permission() -> Result<()> {
        if unsafe { AXIsProcessTrusted() } {
            Ok(())
        } else {
            Err(anyhow!(
                "Accessibility permission is not granted to this terminal app.\n\
                 Open System Settings → Privacy & Security → Accessibility,\n\
                 and enable it for your terminal (Terminal.app, iTerm2, etc.),\n\
                 then re-run this command."
            ))
        }
    }

    fn role(el: &AXUIElement) -> String {
        el.role().map(|s| s.to_string()).unwrap_or_default()
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
        let window = windows
            .iter()
            .find(|w| attr_as_string(w, "AXIdentifier").as_deref() == Some("Main Window"))
            .map(|w| w.clone())
            .ok_or_else(|| {
                anyhow!(
                    "could not find KakaoTalk's main chat-list window. Make sure it's open, not \
                     minimized, and on the Space (virtual desktop) you're currently viewing — the \
                     Accessibility API only sees windows that are visible on the active Space, and \
                     restoring a minimized/off-Space window automatically risks stealing your \
                     foreground focus, which this tool never does. One-time fix if this keeps \
                     happening: right-click the KakaoTalk Dock icon → Options → \
                     Assign To → All Desktops."
                )
            })?;

        // Note: a minimized window still shows up here (unlike one on another
        // Space, which disappears from `windows()` entirely), but restoring
        // it via AXMinimized=false was observed to sometimes bring KakaoTalk
        // to the foreground — which this tool must never do — so we
        // deliberately do NOT auto-restore. The caller gets the same "not
        // found" error and a manual fix, same as the off-Space case.
        let minimized_attr: AXAttribute<CFType> = AXAttribute::new(&CFString::new("AXMinimized"));
        let is_minimized = window
            .attribute(&minimized_attr)
            .ok()
            .and_then(|v| v.downcast::<CFBoolean>())
            .map(bool::from)
            == Some(true);
        if is_minimized {
            return Err(anyhow!(
                "KakaoTalk's main chat-list window is minimized. Restoring it automatically risks \
                 stealing your foreground focus, which this tool never does — please un-minimize \
                 it yourself (click its Dock icon) and retry."
            ));
        }

        Ok(window)
    }

    /// A single recursive snapshot of an AX subtree, capturing each node's
    /// role/value/help/description once so later lookups (`find_first`,
    /// `find_all`) run entirely in memory instead of re-walking the tree via
    /// AX's cross-process IPC on every call. Building this snapshot costs
    /// roughly the same as one `find_descendants_by_role` call; the win is
    /// not calling `find_descendants_by_role` dozens of times against
    /// overlapping subtrees, which is what made `open_chat_row` take ~9s
    /// against an 84-row chat list before this change.
    struct AxNode {
        element: AXUIElement,
        role: String,
        value: Option<String>,
        help: Option<String>,
        description: Option<String>,
        children: Vec<AxNode>,
    }

    /// Build an `AxNode` tree rooted at `root` with one recursive walk,
    /// fetching every node's role, children, value, help, and description in
    /// a **single** `AXUIElementCopyMultipleAttributeValues` IPC round-trip
    /// instead of 2–5 separate `AXUIElementCopyAttributeValue` calls. Each AX
    /// call is a cross-process round-trip to KakaoTalk, so on its ~700-node
    /// main window collapsing five calls into one roughly halves the wall
    /// time of the walk (measured ~4.3ms/node with the old per-attribute
    /// approach). Attributes a node doesn't carry come back as error
    /// placeholders in the same call (`options = 0`, i.e. don't stop on the
    /// first missing one), so they cost no extra round-trip and simply fail
    /// the `downcast` to `None`.
    fn snapshot(root: &AXUIElement) -> AxNode {
        // Order matters: these indices are read back positionally below.
        let names = CFArray::from_CFTypes(&[
            CFString::new("AXRole").as_CFType(),
            CFString::new("AXChildren").as_CFType(),
            CFString::new("AXValue").as_CFType(),
            CFString::new("AXHelp").as_CFType(),
            CFString::new("AXDescription").as_CFType(),
        ]);

        let mut values_ref: CFArrayRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyMultipleAttributeValues(
                root.as_concrete_TypeRef(),
                names.as_concrete_TypeRef(),
                0, // don't stop on error — missing attrs return placeholders
                &mut values_ref,
            )
        };
        if err != 0 || values_ref.is_null() {
            // Rare: the batch call failed for this element. Fall back to a
            // leaf node carrying just the role via the slow per-attr path,
            // so one failed call doesn't drop the whole subtree.
            return AxNode {
                element: root.clone(),
                role: role(root),
                value: None,
                help: None,
                description: None,
                children: Vec::new(),
            };
        }
        let values = unsafe { CFArray::<CFType>::wrap_under_create_rule(values_ref) };

        let string_at = |i: isize| -> Option<String> {
            values
                .get(i)
                .and_then(|v| v.downcast::<CFString>())
                .map(|s| s.to_string())
        };

        // Slot 1 is the AXChildren array. `ConcreteCFType` is only implemented
        // for the untyped `CFArray<*const c_void>`, so downcast to that and
        // wrap each raw element ref as an `AXUIElement` under the get rule
        // (retain), the same +1 retain semantics the typed `.children()`
        // accessor gives. A node with no children yields an error placeholder
        // that fails the array downcast → empty Vec.
        let node_children = values
            .get(1)
            .and_then(|v| v.downcast::<CFArray<*const std::ffi::c_void>>())
            .map(|arr| {
                arr.iter()
                    .map(|child_ref| {
                        let child = unsafe {
                            AXUIElement::wrap_under_get_rule(*child_ref as AXUIElementRef)
                        };
                        snapshot(&child)
                    })
                    .collect()
            })
            .unwrap_or_default();

        AxNode {
            element: root.clone(),
            role: string_at(0).unwrap_or_default(),
            value: string_at(2),
            help: string_at(3),
            description: string_at(4),
            children: node_children,
        }
    }

    impl AxNode {
        /// First descendant (pre-order, self included) with the given role —
        /// same traversal order `find_descendants_by_role(...).first()` used,
        /// just resolved from the in-memory tree instead of a fresh AX walk.
        fn find_first(&self, target_role: &str) -> Option<&AxNode> {
            if self.role == target_role {
                return Some(self);
            }
            for child in &self.children {
                if let Some(found) = child.find_first(target_role) {
                    return Some(found);
                }
            }
            None
        }

        /// All descendants (pre-order, self included) with the given role.
        fn find_all<'a>(&'a self, target_role: &str, out: &mut Vec<&'a AxNode>) {
            if self.role == target_role {
                out.push(self);
            }
            for child in &self.children {
                child.find_all(target_role, out);
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
    ///
    /// Returns the AxNode snapshot to use afterward — either the one just
    /// taken (if the table was already visible) or a fresh one (if the tab
    /// was just pressed, since that changes the UI). Returning the snapshot
    /// instead of re-taking it in the caller avoids a second full tree walk
    /// in the common case (already on the right tab), which previously
    /// doubled open_chat_row's cost.
    fn ensure_chatrooms_tab(main_window: &AXUIElement) -> AxNode {
        let snap = snapshot(main_window);
        if snap.find_first("AXTable").is_some() {
            return snap;
        }
        let mut buttons = Vec::new();
        snap.find_all("AXButton", &mut buttons);
        if let Some(tab) = buttons
            .iter()
            .find(|b| attr_as_string(&b.element, "AXIdentifier").as_deref() == Some("chatrooms"))
        {
            let _ = tab.element.perform_action(&CFString::new(kAXPressAction));
            sleep(Duration::from_millis(400));
            return snapshot(main_window);
        }
        snap
    }

    fn open_chat_row(app: &AXUIElement, chat_display_name: &str) -> Result<()> {
        let debug = std::env::var("OPENKAKAO_CLI_DEBUG").is_ok();
        let start = Instant::now();

        let main_window = find_main_window(app)?;
        let snap = ensure_chatrooms_tab(&main_window);
        if debug {
            eprintln!(
                "[ax_send] open_chat_row: snapshot took {:?}",
                start.elapsed()
            );
        }
        let table = snap
            .find_first("AXTable")
            .ok_or_else(|| anyhow!("could not find chat list table in KakaoTalk's AX tree"))?;

        let mut rows = Vec::new();
        table.find_all("AXRow", &mut rows);

        // Match on the row's first AXStaticText (the chat name) exactly, not
        // a substring — e.g. "Alice" must not accidentally match an
        // "Alice & Bob" group chat. If more than one row has the exact same
        // display name, refuse to guess rather than silently picking one
        // (there is no chat-id to disambiguate with — see
        // SafetyConfig::allowed_send_chats). The matching decision itself is
        // `super::match_chat_row`, a pure function tested outside this
        // macOS-only module.
        let row_names: Vec<Option<String>> = rows
            .iter()
            .map(|row| row.find_first("AXStaticText").and_then(|t| t.value.clone()))
            .collect();

        let row = match super::match_chat_row(&row_names, chat_display_name) {
            super::ChatMatch::NotFound => {
                return Err(anyhow!(
                    "chat '{chat_display_name}' not found in visible/loaded chat list"
                ))
            }
            super::ChatMatch::Found(idx) => rows[idx],
            super::ChatMatch::Ambiguous(count) => {
                return Err(anyhow!(
                    "chat name '{chat_display_name}' matches {count} chats in the visible list — ambiguous, refusing to guess"
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
        let one_row = CFArray::from_CFTypes(std::slice::from_ref(&row.element));
        table
            .element
            .set_attribute(&selected_rows_attr, one_row.as_CFType())
            .map_err(|e| anyhow!("failed to select chat row: {e:?}"))?;

        if debug {
            eprintln!("[ax_send] open_chat_row: total {:?}", start.elapsed());
        }
        Ok(())
    }

    /// Search a single root (a window, or the whole app as a fallback) for the
    /// message composer: an `AXScrollArea` that wraps an `AXTextArea` but no
    /// `AXTable` (which would make it the message list instead).
    fn find_input_field_in(root: &AXUIElement) -> Option<AXUIElement> {
        let snap = snapshot(root);
        let mut scroll_areas = Vec::new();
        snap.find_all("AXScrollArea", &mut scroll_areas);

        for area in scroll_areas {
            if area.find_first("AXTable").is_some() {
                continue; // this scroll area is the message list, not the composer
            }
            if let Some(field) = area.find_first("AXTextArea") {
                return Some(field.element.clone());
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

    /// Scrape every message bubble currently rendered in a chat window's
    /// message list, in on-screen (chronological) order. A row with an
    /// `AXTextArea` is a text message; a row with no `AXTextArea` but an
    /// `AXImage` descendant becomes the placeholder "[사진]"; a row with a
    /// share-labeled `AXButton` ("공유") becomes "[파일]". Rows matching none
    /// of these (date separators, system notices) are skipped, same as
    /// before.
    fn read_visible_messages(window: &AXUIElement) -> Vec<AxMessage> {
        let snap = snapshot(window);
        let Some(table) = snap.find_first("AXTable") else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        table.find_all("AXRow", &mut rows);

        rows.iter()
            .filter_map(|row| {
                let text = message_row_text(row)?;

                let time = row
                    .find_first("AXStaticText")
                    .and_then(|t| t.help.clone().or_else(|| t.value.clone()));

                Some(AxMessage { time, text })
            })
            .collect()
    }

    /// Classify one message row into displayable text: the row's own
    /// `AXTextArea` value if present, else a placeholder if the row looks
    /// like an image or file share, else `None` (not a real message row).
    fn message_row_text(row: &AxNode) -> Option<String> {
        if let Some(text_area) = row.find_first("AXTextArea") {
            if let Some(text) = &text_area.value {
                return Some(text.clone());
            }
        }

        if row.find_first("AXImage").is_some() {
            return Some("[사진]".to_string());
        }

        let mut buttons = Vec::new();
        row.find_all("AXButton", &mut buttons);
        if buttons
            .iter()
            .any(|b| b.description.as_deref() == Some("공유"))
        {
            return Some("[파일]".to_string());
        }

        None
    }

    /// Scrape the text of every message bubble currently rendered in a chat
    /// window's message list, in on-screen (chronological) order.
    fn read_visible_message_texts(window: &AXUIElement) -> Vec<String> {
        read_visible_messages(window)
            .into_iter()
            .map(|m| m.text)
            .collect()
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
        let debug = std::env::var("OPENKAKAO_CLI_DEBUG").is_ok();
        let start = Instant::now();
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = AXUIElement::application(pid);

        open_chat_row(&app, chat_display_name)?;
        press_return(pid)?;

        let deadline = Instant::now() + OPEN_CHAT_TIMEOUT;
        let mut messages = loop {
            if let Some(window) = find_chat_window(&app, chat_display_name) {
                let msgs = read_visible_messages(&window);
                if !msgs.is_empty() {
                    break msgs;
                }
            }
            if Instant::now() >= deadline {
                anyhow::bail!("chat window did not open (or has no visible messages) in time");
            }
            sleep(Duration::from_millis(150));
        };
        if messages.len() > count {
            messages = messages.split_off(messages.len() - count);
        }
        if debug {
            eprintln!("[ax_send] read_via_ax: total {:?}", start.elapsed());
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
        ensure_ax_permission()?;
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

    /// One chat-list row scraped from the main window, read-only (never opens
    /// the chat, so its unread state is untouched).
    #[derive(Debug, Clone)]
    pub struct ChatListRow {
        pub name: String,
        pub unread: i32,
        pub preview: String,
        // Scraped for completeness but not currently consumed by any caller
        // (ax-watch's event doesn't need the row's own last-message
        // timestamp); keep it available for future use.
        #[allow(dead_code)]
        pub timestamp: String,
    }

    /// Scrape every visible/loaded chat-list row from KakaoTalk's main window.
    /// Uses the same single-snapshot chat-list traversal as `open_chat_row`
    /// (main window → chatrooms tab → AXTable → AXRow), but only reads each
    /// row instead of selecting it — so nothing is opened and no unread state
    /// changes. Rows with no readable name are skipped.
    pub fn scrape_chat_list() -> Result<Vec<ChatListRow>> {
        let pid = find_kakaotalk_pid()?;
        ensure_ax_permission()?;
        let app = AXUIElement::application(pid);
        let main_window = find_main_window(&app)?;
        let snap = ensure_chatrooms_tab(&main_window);
        let table = snap
            .find_first("AXTable")
            .ok_or_else(|| anyhow!("could not find chat list table in KakaoTalk's AX tree"))?;

        let mut rows = Vec::new();
        table.find_all("AXRow", &mut rows);

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut static_texts = Vec::new();
            row.find_all("AXStaticText", &mut static_texts);
            // The first static text is the chat name (same convention as
            // open_chat_row). Skip rows we can't name.
            let Some(name) = static_texts.first().and_then(|t| t.value.clone()) else {
                continue;
            };
            // Among the remaining static texts, the unread badge is the one
            // whose whole value parses as an integer (e.g. "5"); a
            // non-numeric one (e.g. "어제", "오후 3:14") is the timestamp.
            let mut unread = 0;
            let mut timestamp = String::new();
            for t in static_texts.iter().skip(1) {
                let Some(v) = t.value.as_deref() else {
                    continue;
                };
                if let Ok(n) = v.trim().parse::<i32>() {
                    if unread == 0 {
                        unread = n;
                    }
                } else if timestamp.is_empty() {
                    timestamp = v.to_string();
                }
            }
            // The last-message preview is the row's AXTextArea value.
            let preview = row
                .find_first("AXTextArea")
                .and_then(|t| t.value.clone())
                .unwrap_or_default();

            out.push(ChatListRow {
                name,
                unread,
                preview,
                timestamp,
            });
        }
        Ok(out)
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
} // mod imp

#[cfg(target_os = "macos")]
pub use imp::{read_via_ax, scrape_chat_list, send_via_ax, ChatListRow};

#[cfg(not(target_os = "macos"))]
mod stub {
    use anyhow::{anyhow, Result};

    /// Mirrors `imp::AxMessage`'s shape so callers don't need cfg-gating.
    /// Never actually constructed here — `read_via_ax` below always errors
    /// on this platform — so its fields would otherwise trip `dead_code`.
    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub struct AxMessage {
        pub time: Option<String>,
        pub text: String,
    }

    pub fn send_via_ax(_chat_display_name: &str, _message: &str) -> Result<()> {
        Err(anyhow!(
            "local-send (AX automation) is only supported on macOS"
        ))
    }

    pub fn read_via_ax(_chat_display_name: &str, _count: usize) -> Result<Vec<AxMessage>> {
        Err(anyhow!(
            "ax-read (AX automation) is only supported on macOS"
        ))
    }

    /// Mirrors `imp::ChatListRow`. Never constructed off macOS (the fn below
    /// always errors), so its fields would otherwise trip `dead_code`.
    #[allow(dead_code)]
    #[derive(Debug, Clone)]
    pub struct ChatListRow {
        pub name: String,
        pub unread: i32,
        pub preview: String,
        pub timestamp: String,
    }

    pub fn scrape_chat_list() -> Result<Vec<ChatListRow>> {
        Err(anyhow!(
            "ax-watch (AX automation) is only supported on macOS"
        ))
    }
}

#[cfg(not(target_os = "macos"))]
pub use stub::{read_via_ax, scrape_chat_list, send_via_ax, ChatListRow};
