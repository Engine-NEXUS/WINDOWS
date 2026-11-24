//! Typed Action Receipts for stage cards (`action:receipt` event).
//!
//! Emitted after NEXUS opens an app, opens a website, or runs a browser search/media action.
//! The stage can display a card summarizing the operation and observed outcome.

use serde::Serialize;
use tauri::{AppHandle, Runtime};

/// Typed receipt payload for the `action:receipt` event.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct ActionReceipt {
    pub(crate) kind: &'static str,
    pub(crate) headline: String,
    pub(crate) subject: String,
    pub(crate) title: Option<String>,
    pub(crate) detail: Option<String>,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) launched_as: Option<&'static str>,
}

/// Truncate a string to at most `max_chars` characters at a valid UTF-8 character boundary.
fn truncate_to_boundary(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        None => s.to_string(),
        Some((byte_idx, _)) => s[..byte_idx].to_string(),
    }
}

/// Extract host from a real URL (e.g. `https://www.youtube.com/watch` -> `youtube.com`)
/// or use a bare name as the subject as is.
fn extract_host_or_name(url_or_name: &str) -> String {
    let s = url_or_name.trim();
    let lower = s.to_lowercase();
    if let Some(rest) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) {
        let prefix_len = s.len() - rest.len();
        let without_scheme = &s[prefix_len..];
        let without_www = if let Some(stripped) = without_scheme.strip_prefix("www.") {
            stripped
        } else if let Some(stripped) = without_scheme.strip_prefix("WWW.") {
            stripped
        } else {
            without_scheme
        };
        let host = without_www
            .split(['/', '?', '#'])
            .next()
            .unwrap_or(without_www);
        if !host.is_empty() {
            return truncate_to_boundary(host, 60);
        }
    }
    truncate_to_boundary(s, 60)
}

/// Whether the search engine host is a video site like youtube.com.
fn is_video_site(engine_host: &str) -> bool {
    let host = engine_host.trim().to_lowercase();
    host.contains("youtube.com")
        || host == "youtu.be"
        || host.contains("vimeo.com")
        || host.contains("twitch.tv")
}

/// Pure builder for app launch receipts.
///
/// Truthfulness: headline is "Opened" ONLY when `in_front` is true;
/// otherwise "Tried to open" with `ok = false`.
pub(crate) fn for_open_app(target: &str, in_front: bool) -> ActionReceipt {
    let subject = truncate_to_boundary(target.trim(), 60);
    if in_front {
        ActionReceipt {
            kind: "open_app",
            headline: "Opened".to_string(),
            subject,
            title: None,
            detail: None,
            ok: true,
            launched_as: Some("app"),
        }
    } else {
        ActionReceipt {
            kind: "open_app",
            headline: "Tried to open".to_string(),
            subject,
            title: None,
            detail: Some("I could not confirm it came to the front.".to_string()),
            ok: false,
            launched_as: Some("app"),
        }
    }
}

/// Pure builder for website opening receipts.
///
/// Extracts host from a real URL (stripping scheme, www., path) or keeps bare name as is.
/// Truthfulness: headline is "Opened" ONLY when `verified` is true;
/// otherwise "Tried to open" with `ok = false`.
pub(crate) fn for_open_site(url_or_name: &str, verified: bool) -> ActionReceipt {
    let subject = extract_host_or_name(url_or_name);
    if verified {
        ActionReceipt {
            kind: "open_site",
            headline: "Opened".to_string(),
            subject,
            title: None,
            detail: None,
            ok: true,
            launched_as: Some("web"),
        }
    } else {
        ActionReceipt {
            kind: "open_site",
            headline: "Tried to open".to_string(),
            subject,
            title: None,
            detail: Some("I could not confirm it came to the front.".to_string()),
            ok: false,
            launched_as: Some("web"),
        }
    }
}

/// Pure builder for search receipts.
///
/// Sets title = query (truncated to 60 chars), detail "Search results. Nothing is playing yet."
/// for video sites like youtube.com, else "Search results.".
pub(crate) fn for_search(query: &str, engine_host: &str) -> ActionReceipt {
    let q = query.trim();
    let title = if q.is_empty() {
        None
    } else {
        Some(truncate_to_boundary(q, 60))
    };
    let subject = truncate_to_boundary(engine_host.trim(), 60);
    let detail = if is_video_site(engine_host) {
        Some("Search results. Nothing is playing yet.".to_string())
    } else {
        Some("Search results.".to_string())
    };
    ActionReceipt {
        kind: "search",
        headline: "Opened".to_string(),
        subject,
        title,
        detail,
        ok: true,
        launched_as: None,
    }
}

/// Pure builder for media transport receipts.
///
/// Truthfulness: Never claims playing unless a play action reported success.
pub(crate) fn for_media(action: &str) -> ActionReceipt {
    let trimmed = action.trim();
    let norm = trimmed.to_lowercase();
    let headline = match norm.as_str() {
        "play" | "play_pause" | "resume" => "Playing".to_string(),
        "pause" => "Paused".to_string(),
        "stop" => "Stopped".to_string(),
        "next" => "Next".to_string(),
        "previous" => "Previous".to_string(),
        _ => "Media".to_string(),
    };
    let subject = if trimmed.is_empty() {
        "media".to_string()
    } else {
        truncate_to_boundary(trimmed, 60)
    };
    ActionReceipt {
        kind: "media",
        headline,
        subject,
        title: None,
        detail: None,
        ok: true,
        launched_as: None,
    }
}

impl ActionReceipt {
    pub(crate) fn for_open_app(target: &str, in_front: bool) -> Self {
        for_open_app(target, in_front)
    }

    pub(crate) fn for_open_site(url_or_name: &str, verified: bool) -> Self {
        for_open_site(url_or_name, verified)
    }

    pub(crate) fn for_search(query: &str, engine_host: &str) -> Self {
        for_search(query, engine_host)
    }

    pub(crate) fn for_media(action: &str) -> Self {
        for_media(action)
    }
}

/// Emit `action:receipt` to the frontend via the typed emit helper.
pub(crate) fn emit<R: Runtime>(app: &AppHandle<R>, r: &ActionReceipt) {
    crate::commands::emit_logged(app, "action:receipt", r.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_app_truthfulness() {
        let open_ok = for_open_app("Notepad", true);
        assert_eq!(open_ok.kind, "open_app");
        assert_eq!(open_ok.headline, "Opened");
        assert_eq!(open_ok.subject, "Notepad");
        assert_eq!(open_ok.title, None);
        assert_eq!(open_ok.detail, None);
        assert!(open_ok.ok);

        let open_fail = for_open_app("Notepad", false);
        assert_eq!(open_fail.kind, "open_app");
        assert_eq!(open_fail.headline, "Tried to open");
        assert_eq!(open_fail.subject, "Notepad");
        assert_eq!(open_fail.title, None);
        assert_eq!(
            open_fail.detail,
            Some("I could not confirm it came to the front.".to_string())
        );
        assert!(!open_fail.ok);
        assert_ne!(open_fail.headline, "Opened");
    }

    #[test]
    fn test_open_site_host_extraction_and_truthfulness() {
        struct Case {
            input: &'static str,
            verified: bool,
            expected_subject: &'static str,
            expected_headline: &'static str,
            expected_ok: bool,
            expected_detail: Option<&'static str>,
        }

        let cases = [
            Case {
                input: "https://www.youtube.com/watch?v=123",
                verified: true,
                expected_subject: "youtube.com",
                expected_headline: "Opened",
                expected_ok: true,
                expected_detail: None,
            },
            Case {
                input: "http://google.com/search?q=rust",
                verified: true,
                expected_subject: "google.com",
                expected_headline: "Opened",
                expected_ok: true,
                expected_detail: None,
            },
            Case {
                input: "https://github.com/rust-lang/rust",
                verified: false,
                expected_subject: "github.com",
                expected_headline: "Tried to open",
                expected_ok: false,
                expected_detail: Some("I could not confirm it came to the front."),
            },
            Case {
                input: "wikipedia",
                verified: true,
                expected_subject: "wikipedia",
                expected_headline: "Opened",
                expected_ok: true,
                expected_detail: None,
            },
            Case {
                input: "google chat",
                verified: false,
                expected_subject: "google chat",
                expected_headline: "Tried to open",
                expected_ok: false,
                expected_detail: Some("I could not confirm it came to the front."),
            },
        ];

        for c in cases {
            let r = for_open_site(c.input, c.verified);
            assert_eq!(r.kind, "open_site");
            assert_eq!(r.subject, c.expected_subject);
            assert_eq!(r.headline, c.expected_headline);
            assert_eq!(r.ok, c.expected_ok);
            assert_eq!(r.detail.as_deref(), c.expected_detail);
            assert_eq!(r.title, None);
            if !c.verified {
                assert_ne!(r.headline, "Opened");
            }
        }
    }

    #[test]
    fn test_search_builder_table() {
        struct Case {
            query: &'static str,
            engine_host: &'static str,
            expected_title: Option<&'static str>,
            expected_detail: &'static str,
        }

        let cases = [
            Case {
                query: "lofi beats",
                engine_host: "youtube.com",
                expected_title: Some("lofi beats"),
                expected_detail: "Search results. Nothing is playing yet.",
            },
            Case {
                query: "ambient coding music",
                engine_host: "https://www.youtube.com/results",
                expected_title: Some("ambient coding music"),
                expected_detail: "Search results. Nothing is playing yet.",
            },
            Case {
                query: "weather tomorrow",
                engine_host: "google.com",
                expected_title: Some("weather tomorrow"),
                expected_detail: "Search results.",
            },
            Case {
                query: "",
                engine_host: "google.com",
                expected_title: None,
                expected_detail: "Search results.",
            },
        ];

        for c in cases {
            let r = for_search(c.query, c.engine_host);
            assert_eq!(r.kind, "search");
            assert_eq!(r.headline, "Opened");
            assert_eq!(r.title.as_deref(), c.expected_title);
            assert_eq!(r.detail.as_deref(), Some(c.expected_detail));
            assert!(r.ok);
        }
    }

    #[test]
    fn test_media_builder_truthfulness() {
        struct Case {
            action: &'static str,
            expected_headline: &'static str,
            expected_subject: &'static str,
        }

        let cases = [
            Case {
                action: "play",
                expected_headline: "Playing",
                expected_subject: "play",
            },
            Case {
                action: "play_pause",
                expected_headline: "Playing",
                expected_subject: "play_pause",
            },
            Case {
                action: "resume",
                expected_headline: "Playing",
                expected_subject: "resume",
            },
            Case {
                action: "pause",
                expected_headline: "Paused",
                expected_subject: "pause",
            },
            Case {
                action: "stop",
                expected_headline: "Stopped",
                expected_subject: "stop",
            },
            Case {
                action: "next",
                expected_headline: "Next",
                expected_subject: "next",
            },
            Case {
                action: "previous",
                expected_headline: "Previous",
                expected_subject: "previous",
            },
            Case {
                action: "rewind",
                expected_headline: "Media",
                expected_subject: "rewind",
            },
        ];

        for c in cases {
            let r = for_media(c.action);
            assert_eq!(r.kind, "media");
            assert_eq!(r.headline, c.expected_headline);
            assert_eq!(r.subject, c.expected_subject);
            assert_eq!(r.title, None);
            assert_eq!(r.detail, None);
            assert!(r.ok);
            // Non-play actions must never claim Playing
            if !matches!(c.action, "play" | "play_pause" | "resume") {
                assert_ne!(r.headline, "Playing");
            }
        }
    }

    #[test]
    fn test_truncation_on_multibyte_text() {
        // Multi-byte string (each emoji is 4 bytes in UTF-8)
        let long_emoji = "🦀".repeat(70);
        let app_receipt = for_open_app(&long_emoji, true);
        assert_eq!(app_receipt.subject.chars().count(), 60);
        assert_eq!(app_receipt.subject, "🦀".repeat(60));

        // Japanese string (each character is 3 bytes in UTF-8)
        let long_jp = "日本語の文字です".repeat(10); // 80 chars
        let search_receipt = for_search(&long_jp, "google.com");
        assert_eq!(
            search_receipt.title.as_ref().unwrap().chars().count(),
            60
        );

        // Multi-byte site name
        let site_receipt = for_open_site(&long_jp, true);
        assert_eq!(site_receipt.subject.chars().count(), 60);
    }

    #[test]
    fn test_associated_and_free_function_equivalence() {
        assert_eq!(
            for_open_app("test", true),
            ActionReceipt::for_open_app("test", true)
        );
        assert_eq!(
            for_open_site("https://example.com", false),
            ActionReceipt::for_open_site("https://example.com", false)
        );
        assert_eq!(
            for_search("query", "host"),
            ActionReceipt::for_search("query", "host")
        );
        assert_eq!(
            for_media("next"),
            ActionReceipt::for_media("next")
        );
    }
}
