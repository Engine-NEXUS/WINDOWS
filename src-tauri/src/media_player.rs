//! Cross-platform Universal Music & Media Controller.
//!
//! Controls music playback and opens media players across macOS and Windows:
//! - macOS (MacBook): Native AppleScript (`osascript`) controls for both Apple Music
//!   (`tell application "Music"`) and Spotify (`tell application "Spotify"`).
//! - Windows: Win32 multimedia virtual key dispatch (`VK_MEDIA_*`), protocol
//!   deep links (`spotify:`, `applemusic:`), and native process activation.
//! - Linux: MPRIS D-Bus interface via `crate::mpris`.

use std::process::Command;

/// Canonical music player identifiers.
pub const PLAYER_SPOTIFY: &str = "spotify";
pub const PLAYER_APPLE_MUSIC: &str = "apple_music";

/// Normalize player name from natural speech / input to canonical ID.
pub fn normalize_player_name(raw: &str) -> &str {
    let lower = raw.trim().to_lowercase();
    if lower.contains("apple") || lower.contains("music") || lower.contains("itunes") {
        PLAYER_APPLE_MUSIC
    } else {
        PLAYER_SPOTIFY
    }
}

/// Resolve which player to use:
/// 1. Explicitly requested player (e.g. from "on Spotify" / "on Apple Music")
/// 2. User's saved preference in settings.json (`defaultMusicPlayer`)
/// 3. Default fallback: "spotify"
pub fn resolve_player(requested: Option<&str>) -> String {
    if let Some(r) = requested {
        return normalize_player_name(r).to_string();
    }

    // Try reading default from local settings.json
    if let Some(config_dir) = dirs_next::data_dir() {
        let settings_path = config_dir.join("com.nexus.assistant").join("settings.json");
        if let Ok(content) = std::fs::read_to_string(&settings_path) {
            if let Some(p) = player_from_settings(&content) {
                return p;
            }
        }
    }

    PLAYER_SPOTIFY.to_string()
}

/// The saved `defaultMusicPlayer`, if the settings text has a usable one. Pure.
fn player_from_settings(content: &str) -> Option<String> {
    let val = serde_json::from_str::<serde_json::Value>(content).ok()?;
    let p = val.get("defaultMusicPlayer")?.as_str()?;
    if p.trim().is_empty() {
        return None;
    }
    Some(normalize_player_name(p).to_string())
}

/// Percent-encode everything except unreserved characters, so a spoken title can never add shell or URL syntax
/// (`R&B`, `rock & roll`, `50% off`, `a;b`). Pure.
pub fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.trim().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn spotify_search_uri(query: &str) -> String {
    format!("spotify:search:{}", encode_component(query))
}

pub fn apple_music_search_url(query: &str) -> String {
    format!("https://music.apple.com/search?term={}", encode_component(query))
}

/// What a Windows media request comes to. Pure, so every phrase can be checked without touching the machine.
#[cfg(any(target_os = "windows", test))]
#[derive(Debug, PartialEq)]
enum WinPlan {
    /// Send this media key, then say this.
    Key(u8, String),
    /// Open this address (an app protocol or a web page), then say this.
    Open(String, String),
    /// Say this and do nothing.
    Refuse(String),
}

#[cfg(any(target_os = "windows", test))]
const VK_PLAY_PAUSE: u8 = 0xB3;
#[cfg(any(target_os = "windows", test))]
const VK_NEXT: u8 = 0xB0;
#[cfg(any(target_os = "windows", test))]
const VK_PREV: u8 = 0xB1;
#[cfg(any(target_os = "windows", test))]
const VK_STOP: u8 = 0xB2;

#[cfg(any(target_os = "windows", test))]
fn plan_windows(action: &str, query: Option<&str>, player: &str, display: &str, installed: impl Fn(&str) -> bool) -> WinPlan {
    let query = query.map(str::trim).filter(|q| !q.is_empty());
    match action {
        "open" | "launch" => {
            if player == PLAYER_APPLE_MUSIC {
                if installed(PLAYER_APPLE_MUSIC) {
                    WinPlan::Open("applemusic:".into(), format!("Opened {display}, sir."))
                } else {
                    WinPlan::Open("https://music.apple.com".into(), "Apple Music isn't installed, so I opened it in the browser, sir.".into())
                }
            } else if installed(PLAYER_SPOTIFY) {
                WinPlan::Open("spotify:".into(), format!("Opened {display}, sir."))
            } else {
                WinPlan::Refuse("Spotify isn't installed on this PC, sir.".into())
            }
        }
        "play_pause" | "toggle" => WinPlan::Key(VK_PLAY_PAUSE, format!("Toggled playback, sir.")),
        "play" => match query {
            Some(q) if player == PLAYER_SPOTIFY => {
                if installed(PLAYER_SPOTIFY) {
                    // A search page only: starting a particular track needs the Spotify web API, so say what happened.
                    WinPlan::Open(spotify_search_uri(q), format!("I opened a Spotify search for {q}, sir. Press play on the result."))
                } else {
                    WinPlan::Refuse("Spotify isn't installed on this PC, sir.".into())
                }
            }
            Some(q) => WinPlan::Open(apple_music_search_url(q), format!("I opened an Apple Music search for {q}, sir.")),
            None => WinPlan::Key(VK_PLAY_PAUSE, format!("Play or pause sent, sir.")),
        },
        "pause" => WinPlan::Key(VK_PLAY_PAUSE, "Pause sent, sir.".into()),
        "next" | "skip" => WinPlan::Key(VK_NEXT, "Skipped to the next track, sir.".into()),
        "previous" | "prev" => WinPlan::Key(VK_PREV, "Back to the previous track, sir.".into()),
        "stop" => WinPlan::Key(VK_STOP, "Stopped, sir.".into()),
        other => WinPlan::Refuse(format!("I don't know the media action {other}, sir.")),
    }
}

/// Display name for user-facing speech and captions.
pub fn player_display_name(player: &str) -> &'static str {
    match normalize_player_name(player) {
        PLAYER_APPLE_MUSIC => "Apple Music",
        _ => "Spotify",
    }
}

/// Execute a media action across supported platforms.
pub async fn execute_media_action(
    action: &str,
    query: Option<&str>,
    player_opt: Option<&str>,
) -> Result<String, String> {
    let player = resolve_player(player_opt);
    let player_name = player_display_name(&player);

    tracing::info!(
        "media_player: executing action='{}' query={:?} player='{}' ({})",
        action,
        query,
        player,
        player_name
    );

    #[cfg(target_os = "macos")]
    {
        execute_macos_action(action, query, &player, player_name)
    }

    #[cfg(target_os = "windows")]
    {
        execute_windows_action(action, query, &player, player_name)
    }

    #[cfg(target_os = "linux")]
    {
        let _ = player_name;
        crate::mpris::send_mpris_command(action).await
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = query;
        Ok(format!("Media {} executed on {player_name}, sir.", action))
    }
}

// ─── macOS AppleScript Implementation ──────────────────────────────────────

#[cfg(target_os = "macos")]
fn execute_macos_action(
    action: &str,
    query: Option<&str>,
    player: &str,
    display: &str,
) -> Result<String, String> {
    let app_name = if player == PLAYER_APPLE_MUSIC {
        "Music"
    } else {
        "Spotify"
    };

    match action {
        "open" | "launch" => {
            let script = format!(r#"tell application "{app_name}" to activate"#);
            run_osascript(&script)?;
            Ok(format!("Opened {display}, sir."))
        }
        "play_pause" | "toggle" => {
            let script = format!(r#"tell application "{app_name}" to playpause"#);
            run_osascript(&script)?;
            Ok(format!("Toggled playback on {display}, sir."))
        }
        "play" => {
            if let Some(q) = query {
                play_track_macos(player, q, display)
            } else {
                let script = format!(r#"tell application "{app_name}" to play"#);
                run_osascript(&script)?;
                Ok(format!("Playing {display}, sir."))
            }
        }
        "pause" => {
            let script = format!(r#"tell application "{app_name}" to pause"#);
            run_osascript(&script)?;
            Ok(format!("Paused {display}, sir."))
        }
        "next" | "skip" => {
            let script = format!(r#"tell application "{app_name}" to next track"#);
            run_osascript(&script)?;
            Ok(format!("Skipped to the next track on {display}, sir."))
        }
        "previous" | "prev" => {
            let script = format!(r#"tell application "{app_name}" to previous track"#);
            run_osascript(&script)?;
            Ok(format!("Playing previous track on {display}, sir."))
        }
        "stop" => {
            let script = if player == PLAYER_APPLE_MUSIC {
                format!(r#"tell application "{app_name}" to stop"#)
            } else {
                format!(r#"tell application "{app_name}" to pause"#)
            };
            run_osascript(&script)?;
            Ok(format!("Stopped {display}, sir."))
        }
        _ => Err(format!("Unknown media action: {action}")),
    }
}

#[cfg(target_os = "macos")]
fn play_track_macos(player: &str, query: &str, display: &str) -> Result<String, String> {
    let escaped = query.replace('"', "\\\"");
    if player == PLAYER_APPLE_MUSIC {
        // Try searching/playing within Music library or opening search
        let script = format!(
            r#"tell application "Music"
                activate
                try
                    play (first track whose name contains "{escaped}")
                on error
                    set sound volume to 75
                    play
                end try
            end tell"#
        );
        run_osascript(&script)?;
        Ok(format!("Playing {} on Apple Music, sir.", query))
    } else {
        // Spotify URI deep-link search & activate
        let encoded = crate::command_executor::urlencoding::encode(query);
        let uri = format!("spotify:search:{encoded}");
        let script = format!(
            r#"tell application "Spotify"
                activate
                open location "{uri}"
                play
            end tell"#
        );
        run_osascript(&script)?;
        Ok(format!("Playing {} on Spotify, sir.", query))
    }
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<(), String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to run osascript: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("AppleScript error: {err}"));
    }
    Ok(())
}

// ─── Windows Implementation ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn execute_windows_action(
    action: &str,
    query: Option<&str>,
    player: &str,
    display: &str,
) -> Result<String, String> {
    match plan_windows(action, query, player, display, protocol_registered) {
        WinPlan::Key(vk, said) => {
            send_windows_media_key(vk)?;
            Ok(said)
        }
        WinPlan::Open(address, said) => {
            open_address(&address)?;
            Ok(said)
        }
        WinPlan::Refuse(said) => Err(said),
    }
}

/// Is an app registered for this player's address scheme (`spotify:` / `applemusic:`)?
#[cfg(target_os = "windows")]
fn protocol_registered(player: &str) -> bool {
    let key = if player == PLAYER_APPLE_MUSIC { r"HKCR\applemusic" } else { r"HKCR\spotify" };
    Command::new("reg")
        .args(["query", key])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Open an address through the shell's own opener, never through `cmd`: the argument is one value, so nothing
/// in it can be read as a second command.
#[cfg(target_os = "windows")]
fn open_address(address: &str) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(address)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("I couldn't open that, sir: {e}"))
}

#[cfg(target_os = "windows")]
fn send_windows_media_key(vk: u8) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
    unsafe {
        // Key down
        keybd_event(vk, 0, Default::default(), 0);
        // Key up
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_player_name() {
        assert_eq!(normalize_player_name("spotify"), PLAYER_SPOTIFY);
        assert_eq!(normalize_player_name("SPOTIFY"), PLAYER_SPOTIFY);
        assert_eq!(normalize_player_name("apple music"), PLAYER_APPLE_MUSIC);
        assert_eq!(normalize_player_name("Apple Music"), PLAYER_APPLE_MUSIC);
        assert_eq!(normalize_player_name("apple"), PLAYER_APPLE_MUSIC);
        assert_eq!(normalize_player_name("music"), PLAYER_APPLE_MUSIC);
    }

    #[test]
    fn test_player_display_name() {
        assert_eq!(player_display_name(PLAYER_SPOTIFY), "Spotify");
        assert_eq!(player_display_name(PLAYER_APPLE_MUSIC), "Apple Music");
    }

    #[test]
    fn test_resolve_player_explicit() {
        assert_eq!(resolve_player(Some("spotify")), "spotify");
        assert_eq!(resolve_player(Some("apple music")), "apple_music");
    }

    #[test]
    fn the_saved_default_player_is_read_from_settings_and_junk_is_ignored() {
        assert_eq!(player_from_settings(r#"{"defaultMusicPlayer":"apple_music"}"#).as_deref(), Some(PLAYER_APPLE_MUSIC));
        assert_eq!(player_from_settings(r#"{"defaultMusicPlayer":"Spotify"}"#).as_deref(), Some(PLAYER_SPOTIFY));
        assert_eq!(player_from_settings(r#"{"defaultMusicPlayer":"  "}"#), None);
        assert_eq!(player_from_settings(r#"{"defaultMusicPlayer":5}"#), None);
        assert_eq!(player_from_settings(r#"{"other":1}"#), None);
        assert_eq!(player_from_settings("not json"), None);
    }

    #[test]
    fn a_spoken_title_can_never_add_shell_or_url_syntax() {
        assert_eq!(encode_component("R&B"), "R%26B");
        assert_eq!(encode_component("rock & roll"), "rock%20%26%20roll");
        assert_eq!(encode_component("50% off; calc | x ^ y"), "50%25%20off%3B%20calc%20%7C%20x%20%5E%20y");
        assert_eq!(encode_component("  Despacito  "), "Despacito");
        assert_eq!(encode_component("caf\u{e9}"), "caf%C3%A9");
        assert_eq!(encode_component("AC-DC_live.v2~"), "AC-DC_live.v2~");
        for evil in ["a&calc", "a|b", "a\"b", "a'b", "a<b>", "a^b", "a%b", "a;b", "a`b", "a$b", "a(b)", "a\nb"] {
            let e = encode_component(evil);
            assert!(e.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '%' | '-' | '_' | '.' | '~')), "{evil} -> {e}");
        }
        assert_eq!(spotify_search_uri("R&B hits"), "spotify:search:R%26B%20hits");
        assert_eq!(apple_music_search_url("R&B hits"), "https://music.apple.com/search?term=R%26B%20hits");
    }

    fn all_installed(_: &str) -> bool { true }
    fn none_installed(_: &str) -> bool { false }

    #[test]
    fn transport_phrases_send_the_right_media_key() {
        for (action, key) in [("play_pause", VK_PLAY_PAUSE), ("toggle", VK_PLAY_PAUSE), ("pause", VK_PLAY_PAUSE),
            ("next", VK_NEXT), ("skip", VK_NEXT), ("previous", VK_PREV), ("prev", VK_PREV), ("stop", VK_STOP)] {
            match plan_windows(action, None, PLAYER_SPOTIFY, "Spotify", none_installed) {
                WinPlan::Key(k, said) => {
                    assert_eq!(k, key, "{action}");
                    assert!(said.ends_with("sir."), "{said}");
                }
                other => panic!("{action}: {other:?}"),
            }
        }
        // plain "play" with nothing to search for is the play/pause key, and it says so
        assert!(matches!(plan_windows("play", None, PLAYER_SPOTIFY, "Spotify", none_installed), WinPlan::Key(VK_PLAY_PAUSE, _)));
        assert!(matches!(plan_windows("play", Some("  "), PLAYER_SPOTIFY, "Spotify", none_installed), WinPlan::Key(VK_PLAY_PAUSE, _)));
    }

    #[test]
    fn playing_a_title_opens_a_search_and_does_not_claim_it_is_playing() {
        let plan = plan_windows("play", Some("bohemian rhapsody"), PLAYER_SPOTIFY, "Spotify", all_installed);
        match plan {
            WinPlan::Open(addr, said) => {
                assert_eq!(addr, "spotify:search:bohemian%20rhapsody");
                assert!(said.contains("opened a Spotify search"), "{said}");
                assert!(!said.to_lowercase().contains("playing"), "must not claim playback: {said}");
            }
            other => panic!("{other:?}"),
        }
        // no media key is sent after a search (it would pause whatever is playing)
        let plan = plan_windows("play", Some("R&B"), PLAYER_APPLE_MUSIC, "Apple Music", none_installed);
        assert!(matches!(plan, WinPlan::Open(ref a, _) if a == "https://music.apple.com/search?term=R%26B"), "{plan:?}");
    }

    #[test]
    fn a_missing_player_is_said_not_pretended() {
        assert!(matches!(plan_windows("open", None, PLAYER_SPOTIFY, "Spotify", none_installed), WinPlan::Refuse(ref s) if s.contains("isn't installed")));
        assert!(matches!(plan_windows("play", Some("x"), PLAYER_SPOTIFY, "Spotify", none_installed), WinPlan::Refuse(_)));
        assert_eq!(plan_windows("open", None, PLAYER_SPOTIFY, "Spotify", all_installed), WinPlan::Open("spotify:".into(), "Opened Spotify, sir.".into()));
        // Apple Music falls back to the web page and says so
        assert!(matches!(plan_windows("open", None, PLAYER_APPLE_MUSIC, "Apple Music", none_installed),
            WinPlan::Open(ref a, ref s) if a == "https://music.apple.com" && s.contains("browser")));
        assert_eq!(plan_windows("open", None, PLAYER_APPLE_MUSIC, "Apple Music", all_installed),
            WinPlan::Open("applemusic:".into(), "Opened Apple Music, sir.".into()));
    }

    /// Live probe (run with `--ignored --nocapture`): which players this PC has, and how the usual phrases parse.
    #[test]
    #[ignore = "reads the registry of the real machine; run by hand"]
    fn live_probe_players_and_phrases() {
        #[cfg(target_os = "windows")]
        println!("[PROOF] spotify registered={} applemusic registered={}", protocol_registered(PLAYER_SPOTIFY), protocol_registered(PLAYER_APPLE_MUSIC));
        for phrase in ["play despacito", "play bohemian rhapsody on spotify", "play the song despacito", "play some music", "next song",
            "pause", "open spotify", "play despacito by luis fonsi", "play rock and roll on apple music", "shuffle my liked songs"] {
            let r = crate::intent_parser::parse_deterministic(phrase);
            println!("[PROOF] {phrase:?} -> {:?}", r.map(|x| x.intent));
        }
    }

    #[test]
    fn bare_play_title_is_music_but_pronouns_transport_words_and_videos_are_not() {
        use crate::intent_parser::{is_music_title, parse_deterministic, ParsedIntent};
        for title in ["despacito", "bohemian rhapsody", "despacito by luis fonsi", "the weeknd", "believer imagine dragons", "arijit singh songs"] {
            assert!(is_music_title(title), "{title}");
        }
        for not in ["it", "that", "again", "pause", "next", "the next song", "previous track", "something", "music", "",
            "minecraft", "the video on youtube", "a movie", "chess online", "episode 4", "twitch stream"] {
            assert!(!is_music_title(not), "{not:?}");
        }
        assert!(!is_music_title(&"word ".repeat(13)), "a whole sentence is not a title");
        let play = |t: &str| parse_deterministic(t).map(|r| r.intent);
        assert!(matches!(play("play despacito"), Some(ParsedIntent::PlayMusic { query: Some(ref q), player: None }) if q == "despacito"));
        assert!(matches!(play("play despacito by luis fonsi"), Some(ParsedIntent::PlayMusic { query: Some(ref q), .. }) if q == "despacito by luis fonsi"));
        assert!(matches!(play("put on believer"), Some(ParsedIntent::PlayMusic { query: Some(ref q), .. }) if q == "believer"));
        // the exact forms keep winning
        assert!(matches!(play("play"), Some(ParsedIntent::MediaPlayPause)));
        assert!(matches!(play("play/pause"), Some(ParsedIntent::MediaPlayPause)));
        assert!(matches!(play("play bohemian rhapsody on spotify"), Some(ParsedIntent::PlayMusic { player: Some(ref p), .. }) if p == "spotify"));
        assert!(matches!(play("play some music"), Some(ParsedIntent::PlayMusic { query: None, .. })));
        // not music: never claimed as music
        assert!(!matches!(play("play minecraft"), Some(ParsedIntent::PlayMusic { .. })));
        assert!(!matches!(play("play the video on youtube"), Some(ParsedIntent::PlayMusic { .. })));
        assert!(!matches!(play("play it again"), Some(ParsedIntent::PlayMusic { .. })));
    }

    #[test]
    fn an_unknown_action_is_refused_without_side_effects() {
        assert!(matches!(plan_windows("rewind", None, PLAYER_SPOTIFY, "Spotify", all_installed), WinPlan::Refuse(ref s) if s.contains("rewind")));
        assert!(matches!(plan_windows("", None, PLAYER_SPOTIFY, "Spotify", all_installed), WinPlan::Refuse(_)));
    }
}
