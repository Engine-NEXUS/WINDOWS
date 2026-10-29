//! Windows notification-center toast: the channel NEXUS uses to tell the owner about a GitHub failure/recovery.
//!
//! NEXUS has no notification centre of its own: the alert appears as a normal Windows toast (and stays in the
//! Action Center). Clicking it activates NEXUS in-process; `notify_hub::activate` then plays the full-screen
//! expand animation and opens the alert in the CodeRabbit sidebar. The toast carries nothing but the opaque
//! activation id `nexus://notify/open?event=<id>`, resolved through the validated ledger (`notify_activation`).
//! The title/body are the already-sanitised, bounded ledger text and are XML-escaped here.
//!
//! Needs an AppUserModelID registered for the app (done per user under HKCU by `ensure_registered`, nothing else is
//! touched). NOT verified on a device: toast display and click activation depend on Windows notification settings
//! (Focus assist, per-app permission). See docs/changes/171.

/// Which channel delivers an alert. Exactly one; there is no in-app card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Toast,
    /// Toasts are switched off (`notifyNative` false): the alert stays in NEXUS history only.
    HistoryOnly,
}

pub fn channel_for(toast_enabled: bool) -> Channel {
    if toast_enabled {
        Channel::Toast
    } else {
        Channel::HistoryOnly
    }
}

const MAX_TITLE: usize = 120;
const MAX_BODY: usize = 240;

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Toast XML for an alert, or None when the event id is not a ledger id (no activation id can be built).
/// `foreground` activation hands the `launch` text back to the running NEXUS (no protocol launch, so a stale
/// installed copy can never be started by mistake). `urgent` adds `scenario="urgent"`: the first such toast makes
/// Windows ask the owner "Allow important notifications from NEXUS?", and once allowed the banner is shown even
/// while Do Not Disturb is on (the opt-in other apps such as Claude and Codex rely on). Windows decides; NEXUS
/// cannot grant itself the permission.
pub fn toast_xml(event_id: &str, title: &str, body: &str, urgent: bool) -> Option<String> {
    let url = crate::notify_activation::activation_url(event_id)?;
    let cut = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let scenario = if urgent { " scenario=\"urgent\"" } else { "" };
    Some(format!(
        "<toast{scenario} activationType=\"foreground\" launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        xml_escape(&url),
        xml_escape(&cut(title, MAX_TITLE)),
        xml_escape(&cut(body, MAX_BODY)),
    ))
}

/// The Windows identity toasts are shown under.
pub const APP_ID: &str = "com.nexus.assistant";

/// Registry path (under HKCU) that gives the identity a display name so Windows shows the toast as "NEXUS".
pub fn registration_path() -> String {
    format!("Software\\Classes\\AppUserModelId\\{APP_ID}")
}

#[cfg(windows)]
fn ensure_registered() -> Result<(), String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(registration_path()).map_err(|e| e.to_string())?;
    key.set_value("DisplayName", &"NEXUS").map_err(|e| e.to_string())?;
    key.set_value("ShowInSettings", &1u32).map_err(|e| e.to_string())
}

/// Shows the toast. `on_activate` receives the `launch` text when the owner clicks it.
#[cfg(windows)]
pub fn show(xml: &str, on_activate: impl Fn(String) + Send + Sync + 'static) -> Result<(), String> {
    use windows::core::{Interface, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastActivatedEventArgs, ToastNotification, ToastNotificationManager};
    ensure_registered()?;
    let doc = XmlDocument::new().map_err(|e| e.to_string())?;
    doc.LoadXml(HSTRING::from(xml)).map_err(|e| e.to_string())?;
    let toast = ToastNotification::CreateToastNotification(&doc).map_err(|e| e.to_string())?;
    toast
        .Activated(TypedEventHandler::new(move |_sender: &Option<ToastNotification>, args: &Option<windows::core::IInspectable>| {
            if let Some(a) = args.as_ref().and_then(|a| a.cast::<ToastActivatedEventArgs>().ok()) {
                if let Ok(launch) = a.Arguments() {
                    on_activate(launch.to_string());
                }
            }
            Ok(())
        }))
        .map_err(|e| e.to_string())?;
    ToastNotificationManager::CreateToastNotifierWithId(HSTRING::from(APP_ID))
        .map_err(|e| e.to_string())?
        .Show(&toast)
        .map_err(|e| e.to_string())?;
    // The activation handler lives on the toast object: keep it alive for the rest of the session (a few small objects).
    std::mem::forget(toast);
    Ok(())
}

#[cfg(not(windows))]
pub fn show(_xml: &str, _on_activate: impl Fn(String) + Send + Sync + 'static) -> Result<(), String> {
    Err("native toast is not implemented on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ev_0123456789abcdef01234567";

    #[test]
    fn the_toast_is_the_only_channel_unless_switched_off() {
        assert_eq!(channel_for(true), Channel::Toast);
        assert_eq!(channel_for(false), Channel::HistoryOnly);
    }

    #[test]
    fn the_toast_carries_only_the_activation_id_and_escaped_text() {
        let xml = toast_xml(ID, "Build <failed> & \"more\"", "owner/repo's run\u{7}", false).unwrap();
        assert!(xml.contains("activationType=\"foreground\""), "handled by the running app, no protocol launch");
        assert!(xml.contains("launch=\"nexus://notify/open?event=ev_0123456789abcdef01234567\""));
        assert!(xml.contains("Build &lt;failed&gt; &amp; &quot;more&quot;"));
        assert!(xml.contains("owner/repo&apos;s run"));
        assert!(!xml.contains('\u{7}'));
        assert_eq!(xml.matches("<text>").count(), 2);
        assert!(!xml.contains("http"), "no arbitrary URL is ever put in the toast");
    }

    #[test]
    fn only_urgent_toasts_ask_to_pass_do_not_disturb() {
        let urgent = toast_xml(ID, "t", "b", true).unwrap();
        assert!(urgent.starts_with("<toast scenario=\"urgent\" activationType=\"foreground\""));
        let normal = toast_xml(ID, "t", "b", false).unwrap();
        assert!(!normal.contains("scenario"), "a recovery must not break through Do Not Disturb");
    }

    #[test]
    fn a_non_ledger_id_gets_no_toast() {
        assert_eq!(toast_xml("not-an-id", "t", "b", true), None);
        assert_eq!(toast_xml("ev_x\" onclick=\"y", "t", "b", false), None);
    }

    #[test]
    fn text_is_bounded() {
        let xml = toast_xml(ID, &"t".repeat(500), &"b".repeat(900), false).unwrap();
        assert!(xml.contains(&"t".repeat(120)) && !xml.contains(&"t".repeat(121)));
        assert!(xml.contains(&"b".repeat(240)) && !xml.contains(&"b".repeat(241)));
    }

    /// Manual device check (`cargo test --lib -- --ignored manual_toast_smoke --nocapture`): shows a real toast
    /// through the same code the app uses and reports whether Windows put it in the Action Center.
    #[cfg(windows)]
    #[test]
    #[ignore = "shows a real toast on the desktop"]
    fn manual_toast_smoke() {
        use windows::core::HSTRING;
        use windows::UI::Notifications::ToastNotificationManager;
        let xml = toast_xml(ID, "PR failed in selftest (Rust smoke test)", "nexus-test/selftest - run 1", true).unwrap();
        show(&xml, |launch| println!("activated with: {launch}")).expect("toast could not be shown");
        std::thread::sleep(std::time::Duration::from_secs(2));
        let history = ToastNotificationManager::History().expect("history").GetHistoryWithId(HSTRING::from(APP_ID)).expect("history for app");
        let n = history.Size().expect("size");
        println!("Action Center holds {n} toast(s) for {APP_ID}");
        assert!(n >= 1, "Windows did not keep the toast");
    }

    #[test]
    fn the_registration_stays_inside_the_apps_own_key() {
        let p = registration_path();
        assert_eq!(p, "Software\\Classes\\AppUserModelId\\com.nexus.assistant");
        assert!(!p.contains(".."));
    }
}
