//! "Analyse this email": what a vision model reads off the open email, validated in code before anything is
//! shown, spoken or opened. Pure (no network, no window): the runner in `orchestrator` does the capture and
//! the Gemini call, this file owns the prompt, the parsing, the sidebar card and the spoken line.
//!
//! Rule: nothing the model returns is trusted. A venue is only used for a map when it is a short plain-text
//! place, links must be https, and an email with no venue or address never opens a map.

use serde_json::Value;

const MAX_TEXT: usize = 400;
const MAX_PLACE: usize = 160;
const MAX_LIST: usize = 6;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EmailBrief {
    pub title: String,
    pub summary: String,
    pub sender: Option<String>,
    pub dates: Vec<String>,
    pub deadline: Option<String>,
    pub venue: Option<String>,
    pub address: Option<String>,
    pub links: Vec<String>,
    pub actions: Vec<String>,
}

pub fn build_prompt() -> &'static str {
    "You are reading a screenshot of an open email. Reply with JSON only, no prose, in exactly this shape:\n\
{\"is_email\": true, \"title\": \"short subject or event name\", \"summary\": \"two sentences, facts from the email only\", \
\"sender\": \"name or null\", \"dates\": [\"dates and times mentioned, as written\"], \"deadline\": \"registration or submission deadline or null\", \
\"venue\": \"name of the place the event happens, or null\", \"address\": \"street address or city of the venue as written, or null\", \
\"links\": [\"https links shown on screen\"], \"actions\": [\"what the reader must do, short\"]}\n\
Rules: use only what is visible; if something is not in the email use null or []; never guess a venue or address; \
if the screenshot is not an email set is_email to false."
}

fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control() || *c == ' ').collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

/// "null", "none", "n/a" and empty strings are the model saying "not in the email".
fn opt(v: Option<&Value>, max: usize) -> Option<String> {
    let s = clean(v?.as_str()?, max);
    let l = s.to_lowercase();
    if s.is_empty() || matches!(l.as_str(), "null" | "none" | "n/a" | "na" | "not specified" | "unknown" | "not mentioned") {
        None
    } else {
        Some(s)
    }
}

fn list(v: Option<&Value>, max_item: usize, keep: impl Fn(&str) -> bool) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|a| {
            let mut out: Vec<String> = Vec::new();
            for x in a {
                if let Some(s) = opt(Some(x), max_item) {
                    if keep(&s) && !out.contains(&s) {
                        out.push(s);
                    }
                }
                if out.len() >= MAX_LIST {
                    break;
                }
            }
            out
        })
        .unwrap_or_default()
}

/// Parse the model's answer. `None` when it is not an email or has nothing usable.
pub fn parse_brief(text: &str) -> Option<EmailBrief> {
    let v: Value = serde_json::from_str(crate::vision::strip_json_fences(text)).ok()?;
    if v.get("is_email").and_then(|b| b.as_bool()) == Some(false) {
        return None;
    }
    let title = opt(v.get("title"), 140)?;
    let summary = opt(v.get("summary"), MAX_TEXT).unwrap_or_default();
    Some(EmailBrief {
        title,
        summary,
        sender: opt(v.get("sender"), 100),
        dates: list(v.get("dates"), 80, |_| true),
        deadline: opt(v.get("deadline"), 100),
        venue: opt(v.get("venue"), MAX_PLACE),
        address: opt(v.get("address"), MAX_PLACE),
        links: list(v.get("links"), 200, |l| l.starts_with("https://") && !l.contains(' ')),
        actions: list(v.get("actions"), 140, |_| true),
    })
}

/// The text to search on Maps, or `None` when the email names no place. A venue alone or an address alone is
/// enough; both are joined so Maps lands on the right building.
pub fn map_query(b: &EmailBrief) -> Option<String> {
    let place_ok = |s: &str| s.split_whitespace().count() <= 20 && !s.to_lowercase().starts_with("online") && !s.to_lowercase().contains("zoom") && !s.to_lowercase().contains("google meet");
    let venue = b.venue.as_deref().filter(|s| place_ok(s));
    let address = b.address.as_deref().filter(|s| place_ok(s));
    match (venue, address) {
        (Some(v), Some(a)) if !a.to_lowercase().contains(&v.to_lowercase()) => Some(format!("{v}, {a}")),
        (Some(v), Some(_)) => Some(v.to_string()),
        (Some(v), None) => Some(v.to_string()),
        (None, Some(a)) => Some(a.to_string()),
        (None, None) => None,
    }
}

pub fn card_markdown(b: &EmailBrief) -> String {
    let mut md = format!("## {}\n\n", b.title);
    if let Some(s) = &b.sender {
        md.push_str(&format!("**From:** {s}\n\n"));
    }
    if !b.summary.is_empty() {
        md.push_str(&format!("{}\n\n", b.summary));
    }
    if !b.dates.is_empty() {
        md.push_str("**When**\n\n");
        for d in &b.dates {
            md.push_str(&format!("- {d}\n"));
        }
        md.push('\n');
    }
    if let Some(d) = &b.deadline {
        md.push_str(&format!("**Deadline:** {d}\n\n"));
    }
    match (&b.venue, &b.address) {
        (Some(v), Some(a)) => md.push_str(&format!("**Where:** {v}, {a}\n\n")),
        (Some(v), None) => md.push_str(&format!("**Where:** {v}\n\n")),
        (None, Some(a)) => md.push_str(&format!("**Where:** {a}\n\n")),
        (None, None) => {}
    }
    if !b.actions.is_empty() {
        md.push_str("**What to do**\n\n");
        for a in &b.actions {
            md.push_str(&format!("- {a}\n"));
        }
        md.push('\n');
    }
    if !b.links.is_empty() {
        md.push_str("**Links**\n\n");
        for l in &b.links {
            md.push_str(&format!("- {l}\n"));
        }
    }
    md
}

/// One spoken line. Mentions the map only when one was opened.
pub fn spoken_line(b: &EmailBrief, opened_map: bool) -> String {
    let mut line = format!("This email is about {}.", b.title.trim_end_matches('.'));
    if let Some(d) = &b.deadline {
        line.push_str(&format!(" The deadline is {d}."));
    } else if let Some(d) = b.dates.first() {
        line.push_str(&format!(" It mentions {d}."));
    }
    if opened_map {
        line.push_str(" I've opened the location on the map, and the details are in the sidebar.");
    } else {
        line.push_str(" The details are in the sidebar.");
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"```json
{"is_email": true, "title": "Hack4Good 2026 - confirmation", "summary": "You are registered. Check-in opens at 9 am.",
 "sender": "Hack4Good Team", "dates": ["Nov 14-15, 2026", "Check-in 9:00 AM"], "deadline": "Nov 10",
 "venue": "T-Hub", "address": "Raidurgam, Hyderabad", "links": ["https://hack4good.example/rules", "http://insecure.example", "javascript:alert(1)"],
 "actions": ["Bring a laptop", "Bring a laptop", "Carry ID"]}
```"#;

    #[test]
    fn parses_a_full_email_and_keeps_only_safe_parts() {
        let b = parse_brief(GOOD).unwrap();
        assert_eq!(b.title, "Hack4Good 2026 - confirmation");
        assert_eq!(b.venue.as_deref(), Some("T-Hub"));
        assert_eq!(b.links, vec!["https://hack4good.example/rules".to_string()], "only https links survive");
        assert_eq!(b.actions, vec!["Bring a laptop".to_string(), "Carry ID".to_string()], "duplicates removed");
        assert_eq!(map_query(&b).as_deref(), Some("T-Hub, Raidurgam, Hyderabad"));
    }

    #[test]
    fn no_venue_means_no_map() {
        let b = parse_brief(r#"{"is_email": true, "title": "Weekly digest", "summary": "News.", "venue": null, "address": "N/A"}"#).unwrap();
        assert_eq!(map_query(&b), None);
        assert!(spoken_line(&b, false).contains("sidebar") && !spoken_line(&b, false).contains("map"));
    }

    #[test]
    fn online_events_do_not_open_a_map() {
        let b = parse_brief(r#"{"is_email": true, "title": "Webinar", "venue": "Online (Zoom)", "address": null}"#).unwrap();
        assert_eq!(map_query(&b), None);
        let b2 = parse_brief(r#"{"is_email": true, "title": "Call", "venue": "Google Meet link below"}"#).unwrap();
        assert_eq!(map_query(&b2), None);
    }

    #[test]
    fn rejects_non_emails_and_garbage() {
        assert!(parse_brief(r#"{"is_email": false}"#).is_none());
        assert!(parse_brief("sorry I cannot see an email").is_none());
        assert!(parse_brief(r#"{"is_email": true, "title": ""}"#).is_none());
        assert!(parse_brief(r#"{"is_email": true}"#).is_none());
    }

    #[test]
    fn venue_already_inside_the_address_is_not_repeated() {
        let b = EmailBrief { title: "t".into(), venue: Some("T-Hub".into()), address: Some("T-Hub, Raidurgam".into()), ..Default::default() };
        assert_eq!(map_query(&b).as_deref(), Some("T-Hub"));
    }

    #[test]
    fn card_and_line_carry_the_facts() {
        let b = parse_brief(GOOD).unwrap();
        let md = card_markdown(&b);
        assert!(md.contains("## Hack4Good") && md.contains("**Deadline:** Nov 10") && md.contains("**Where:** T-Hub, Raidurgam, Hyderabad"));
        assert!(md.contains("https://hack4good.example/rules") && !md.contains("javascript"));
        let line = spoken_line(&b, true);
        assert!(line.contains("deadline is Nov 10") && line.contains("opened the location"));
    }

    #[test]
    fn control_characters_and_length_are_clamped() {
        let long = "x".repeat(2000);
        let raw = format!("{{\"is_email\": true, \"title\": \"A\\u0007B {long}\", \"summary\": \"{long}\"}}");
        let b = parse_brief(&raw).unwrap();
        assert!(b.title.chars().count() <= 140 && !b.title.contains('\u{7}'));
        assert!(b.summary.chars().count() <= MAX_TEXT);
    }
}
