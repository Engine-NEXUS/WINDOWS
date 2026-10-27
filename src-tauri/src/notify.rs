//! Notification event keys — feature 151 Phase A.
//!
//! PROBLEM: proactive speech is deduplicated per alert id only (`mail:{message-id}`).
//! The same logical event arriving as several emails spoke each time — five CI-failure
//! runs for one repo within minutes, a reply + a follow-up in one thread — which is the
//! user's complaint in feature-151 research: notifications "overlap… again and again".
//!
//! DESIGN: a stable, deterministic key for *the event behind the mail* — no AI, no
//! network, no clock (feature-151 plan §1.2):
//!   1. GitHub-shaped mail: the repeated ENTITY beats thread identity, because CI
//!      failure emails arrive as new message ids AND new threads on every run. Key =
//!      `mail:gh:<owner/repo>:<stem>`, where the stem is the workflow/PR subject left
//!      after stripping tags, run numbers and mail-system noise. Two runs of the same
//!      workflow collide (Phase B counts them up); a different workflow or a review
//!      request does not.
//!   2. Otherwise a real thread id → `mail:thread:<id>`.
//!   3. Fall back to sender local-part + stem → `mail:<localpart>:<stem>`.
//! Phase B (`proactive_policy`) consumes such keys as a 5-minute Medium-tier cooldown.

use crate::memcore::mailwording::extract_github_owner_repo;

/// Cap so pathological subjects cannot bloat keys.
const STEM_MAX_CHARS: usize = 60;

/// Stable identity of the event behind one notification (feature 151 Phase A).
pub fn event_key(source: &str, thread_id: &str, sender: &str, subject: &str) -> String {
    let src = source.trim().to_lowercase();
    if is_github(sender, subject) {
        if let Some((owner, repo)) = extract_github_owner_repo(subject) {
            let entity = format!("{owner}/{repo}").to_lowercase();
            return format!("{src}:gh:{entity}:{}", stem(subject));
        }
    }
    let t = thread_id.trim().to_lowercase();
    if !t.is_empty() && t != "0" {
        return format!("{src}:thread:{t}");
    }
    let who = sender.trim().to_lowercase();
    let local = who.rsplit_once('@').map(|(l, _)| l).unwrap_or(&who);
    let local = if local.is_empty() { "unknown" } else { local };
    format!("{src}:{local}:{}", stem(subject))
}

fn is_github(sender: &str, subject: &str) -> bool {
    sender.to_lowercase().contains("github.com") || subject.to_lowercase().contains("github.com/")
}

/// Subject normalized to a repeat-stable stem: lowercase; `re:`/`fw:`/`fwd:` prefixes
/// off; any `[..]`/`(..)`/`{..}` group off (tags, counters, run numbers); fixed
/// mail-system tails off; punctuation to spaces; whitespace collapsed; capped.
pub fn stem(subject: &str) -> String {
    let s = prefixless(subject.trim().to_lowercase());
    let s = strip_bracket_groups(&s);
    let s = drop_run_refs(&s);
    let s = drop_marker_tokens(&s);
    let s = drop_phrases(&s, &["the github team", "view it on github"]);
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    word_cap(cleaned.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Repeatedly strip reply/forward prefixes (`re:` …).
fn prefixless(mut s: String) -> String {
    loop {
        let lower = s.clone();
        let cut = ["re:", "fw:", "fwd:"]
            .iter()
            .find_map(|p| lower.strip_prefix(p).map(|r| r.trim_start()));
        match cut {
            Some(rest) => s = rest.to_string(),
            None => return s,
        }
    }
}

/// Remove every bracketed group (nested included); mail tags and counters die here.
fn strip_bracket_groups(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '[' | '(' | '{' => depth = depth.saturating_add(1),
            ']' | ')' | '}' => {
                if depth > 0 {
                    depth -= 1;
                    out.push(' ');
                }
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Drop bare `re:`/`fw:`/`fwd:` tokens — reply/forward markers sit mid-subject too
/// (`[tag] Re: …`), so the leading-only prefix strip needs this second pass.
fn drop_marker_tokens(s: &str) -> String {
    s.split_whitespace()
        .filter(|w| !matches!(*w, "re:" | "fw:" | "fwd:"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drop whole `#123`-style tokens (GitHub run / counter references).
fn drop_run_refs(s: &str) -> String {
    s.split_whitespace()
        .filter(|w| !(w.len() > 1 && w.starts_with('#') && w[1..].chars().all(|c| c.is_ascii_digit())))
        .collect::<Vec<_>>()
        .join(" ")
}

fn drop_phrases(s: &str, phrases: &[&str]) -> String {
    let mut out = s.to_string();
    for p in phrases {
        out = out.replace(p, " ");
    }
    out
}

/// Cut the stem at `STEM_MAX_CHARS` on a word boundary; empty input → "event".
fn word_cap(s: String) -> String {
    if s.chars().count() <= STEM_MAX_CHARS {
        return if s.is_empty() { "event".into() } else { s };
    }
    let cut: String = s.chars().take(STEM_MAX_CHARS).collect();
    let trimmed = match cut.rfind(' ') {
        Some(i) if i > 8 => cut[..i].trim_end().to_string(),
        _ => cut.trim_end().to_string(),
    };
    if trimmed.is_empty() { "event".into() } else { trimmed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_github_runs_of_the_same_workflow_get_one_key() {
        let a = event_key("mail", "t1", "notifications@github.com", "[nexus-engine/window] Run failed: Build (#12)");
        let b = event_key("mail", "t2", "notifications@github.com", "[nexus-engine/window] Run failed: Build (failure #13)");
        assert_eq!(a, b, "consecutive failure runs must collide on the event");
        assert!(a.starts_with("mail:gh:nexus-engine/window:"), "got {a}");
    }

    #[test]
    fn different_workflows_and_reviews_give_different_keys() {
        let fail = event_key("mail", "t1", "notifications@github.com", "[nexus-engine/window] Run failed: Build (#12)");
        let test = event_key("mail", "t2", "notifications@github.com", "[nexus-engine/window] Run failed: Test (#12)");
        let review = event_key("mail", "t3", "notifications@github.com", "[nexus-engine/layout] Review requested: Fix login (PR #5)");
        assert_ne!(fail, test);
        assert_ne!(fail, review);
        assert_ne!(test, review);
    }

    #[test]
    fn same_thread_emails_give_one_key() {
        let a = event_key("mail", "Thread7", "news@post.io", "Re: Weekly digest");
        let b = event_key("mail", "thread7", "news@post.io", "RE: Weekly digest — The GitHub Team");
        assert_eq!(a, b);
        assert!(a.starts_with("mail:thread:thread7"), "got {a}");
    }

    #[test]
    fn without_a_thread_sender_and_stem_carry_the_key() {
        let a = event_key("mail", "", "bills@shop.com", "Re: Invoice overdue");
        let b = event_key("mail", "0", "bills@shop.com", "Invoice overdue (2)");
        let c = event_key("mail", "", "support@vendor.com", "Invoice overdue");
        let d = event_key("mail", "", "bills@shop.com", "Shipment delivered");
        assert_eq!(a, b, "a resend of one exchange still collides");
        assert_ne!(a, c, "different senders differ");
        assert_ne!(a, d, "different subjects differ");
        assert!(a.starts_with("mail:bills:invoice overdue"), "got {a}");
        assert_eq!(event_key("mail", "", "bills@shop.com", "").split(':').last(), Some("event"));
    }

    #[test]
    fn stems_strip_tags_counters_and_numbers() {
        assert_eq!(stem("[General] Re: FWD: The CI digest (2) — The GitHub Team"), "the ci digest");
        assert_eq!(stem("[nexus-engine/window] Run failed: Build (#12)"), "run failed build");
        assert_eq!(stem("Invoice #9812 overdue"), "invoice overdue");
    }

    #[test]
    fn long_subjects_cap_at_the_stem_limit() {
        let long = "word ".repeat(30);
        assert!(stem(&long).chars().count() <= STEM_MAX_CHARS);
        assert_eq!(stem(""), "event");
    }
}
