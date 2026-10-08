//! Boot briefing (plan P3): after the laptop starts, NEXUS says where the
//! user left off and what is still open — once a day, only when it has
//! something true to say, never during a meeting.
//!
//! The text is built deterministically from local data (resume points, mail
//! deadlines the user asked to watch, requests NEXUS could not finish). No
//! cloud call is involved, so nothing about it leaves the device. It never
//! invents a quantity: "how much is left" is only ever a count of open
//! items or a deadline taken verbatim from an email.

use std::sync::Mutex;

use super::agenda::AgendaItem;
use super::mailtriage::{self, Category, StoredMail};
use super::resume::{self, Kind, Sample};
use super::store::Tier;

/// Resume points older than this are not "where you left off" any more.
pub const MAX_RESUME_AGE_SECS: i64 = 7 * 24 * 3600;
/// Wait after startup before the briefing is submitted (boot settles first).
pub const BOOT_DELAY_SECS: u64 = 45;
/// The user counts as present with input in the last minute (unknown = assume present).
pub const PRESENT_WITHIN_SECS: u64 = 60;
/// How long the boot briefing waits for the user to show up before giving up for today.
pub const PRESENCE_WAIT_SECS: u64 = 20 * 60;
const PRESENCE_POLL_SECS: u64 = 10;

pub fn user_present(idle_secs: Option<u64>) -> bool {
    idle_secs.map(|s| s < PRESENT_WITHIN_SECS).unwrap_or(true)
}

/// An explicit "where did I leave off" ignores the window the user is in now.
const CURRENT_WINDOW_SECS: i64 = 90;

#[derive(Debug, Clone, PartialEq)]
pub struct ResumeEntry {
    pub sample: Sample,
    pub last_seen: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MailItem {
    pub sender: String,
    pub subject: String,
    /// Deadline text exactly as found in the email ("Friday 5 PM").
    pub deadline: Option<String>,
    /// Inbox-triage category ("exam", "database", …) for mail the watcher filed.
    pub category: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Input {
    pub now: i64,
    /// Local hour of day, 0-23.
    pub hour: u32,
    pub name: Option<String>,
    pub friend: bool,
    /// Newest first.
    pub resume: Vec<ResumeEntry>,
    pub mail: Vec<MailItem>,
    /// Requests NEXUS could not finish, newest first.
    pub unfinished: Vec<String>,
    /// Today's calendar (empty if not connected / unreachable).
    pub events: Vec<AgendaItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Briefing {
    pub spoken: String,
    pub card_md: String,
    /// Open items (mail deadlines + unfinished requests).
    pub items: usize,
}

fn clip(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        return t.to_string();
    }
    let cut: String = t.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

fn md_escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|') {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

pub fn ago_phrase(now: i64, ts: i64) -> String {
    let d = (now - ts).max(0);
    match d {
        0..=89 => "just now".into(),
        90..=3599 => format!("about {} minutes ago", (d / 60).max(2)),
        3600..=86_399 => {
            let h = (d + 1800) / 3600;
            if h == 1 { "about an hour ago".into() } else { format!("about {h} hours ago") }
        }
        86_400..=172_799 => "yesterday".into(),
        _ => format!("{} days ago", d / 86_400),
    }
}

fn greeting(hour: u32, name: Option<&str>, friend: bool) -> String {
    let base = match hour {
        5..=11 => "Good morning",
        12..=16 => "Good afternoon",
        17..=21 => "Good evening",
        _ => "Hello",
    };
    format!("{base}{}.", crate::persona::greet_address(name, friend))
}

fn site_label(url: &str) -> Option<&'static str> {
    let u = url.to_lowercase();
    [
        ("leetcode.com", "LeetCode"),
        ("github.com", "GitHub"),
        ("youtube.com", "YouTube"),
        ("stackoverflow.com", "Stack Overflow"),
        ("docs.google.com", "Google Docs"),
        ("mail.google.com", "Gmail"),
    ]
    .iter()
    .find(|(host, _)| u.contains(host))
    .map(|(_, label)| *label)
}

/// Shell/terminal window titles are paths or executables ("C:\WINDOWS\system32\cmd.exe"),
/// which read out loud as noise.
fn title_is_noise(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    t.contains(":\\") || t.ends_with(".exe") || t.starts_with("administrator:")
}

/// "watching “…” on YouTube", "working on LeetCode: “…”", "in VS Code on “main.rs”".
pub fn describe(s: &Sample) -> String {
    let title = clip(&s.title, 60);
    match resume::kind(s) {
        Kind::Video => {
            let t = title.trim_end_matches(" - YouTube").to_string();
            format!("watching “{t}” on YouTube")
        }
        Kind::Browser => match s.url.as_deref().and_then(site_label) {
            Some(site) => format!("on {site}: “{title}”"),
            None => format!("reading “{title}” in {}", resume::friendly_app(&s.app)),
        },
        Kind::App => {
            let app = resume::friendly_app(&s.app);
            if title.is_empty() || title_is_noise(&title) {
                format!("in {app}")
            } else {
                format!("in {app} on “{title}”")
            }
        }
    }
}

pub fn mail_line(m: &MailItem) -> String {
    let who = clip(&m.sender, 40);
    let subj = clip(&m.subject, 60);
    if let Some(cat) = m.category.as_deref().and_then(Category::from_str) {
        return format!("{} from {who}: “{subj}”", cat.phrase());
    }
    match m.deadline.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => format!("{who} emailed about “{subj}”, deadline {d}"),
        None => format!("{who} emailed about “{subj}”"),
    }
}

/// Build the briefing, or `None` when there is nothing true to say.
pub fn build(input: &Input) -> Option<Briefing> {
    let latest = input
        .resume
        .iter()
        .find(|r| input.now - r.last_seen <= MAX_RESUME_AGE_SECS);
    let mut open: Vec<String> = input.mail.iter().map(mail_line).collect();
    open.extend(
        input
            .unfinished
            .iter()
            .map(|t| format!("I couldn’t finish “{}”", clip(t, 70))),
    );
    if latest.is_none() && open.is_empty() && input.events.is_empty() {
        return None;
    }

    // ── spoken (short) ──
    let mut spoken = greeting(input.hour, input.name.as_deref(), input.friend);
    if let Some(r) = latest {
        spoken.push_str(&format!(
            " You were {}, {}.",
            describe(&r.sample),
            ago_phrase(input.now, r.last_seen)
        ));
    }
    if let Some(first) = input.events.first() {
        let n = input.events.len();
        let when = if first.all_day { "all day".to_string() } else { format!("at {}", first.when_label) };
        spoken.push_str(&if n == 1 {
            format!(" You have one event today: {} {when}.", first.summary)
        } else {
            format!(" You have {n} events today; the first is {} {when}.", first.summary)
        });
    }
    match open.len() {
        0 => {}
        1 => spoken.push_str(&format!(" One thing is open: {}.", open[0])),
        n => {
            spoken.push_str(&format!(" {n} things are open. First: {}.", open[0]));
            if n > 1 {
                spoken.push_str(" Say “show my briefing” for the rest.");
            }
        }
    }

    // ── card (full) ──
    let mut md = String::from("## Welcome back\n\n");
    let recents: Vec<&ResumeEntry> = input
        .resume
        .iter()
        .filter(|r| input.now - r.last_seen <= MAX_RESUME_AGE_SECS)
        .take(3)
        .collect();
    if !recents.is_empty() {
        md.push_str("### Where you left off\n");
        for r in recents {
            md.push_str(&format!(
                "- {} — {}\n",
                md_escape(&describe(&r.sample)),
                ago_phrase(input.now, r.last_seen)
            ));
        }
        md.push('\n');
    }
    if !input.events.is_empty() {
        md.push_str("### Today's calendar\n");
        for e in &input.events {
            md.push_str(&format!("- {} — {}\n", md_escape(&e.summary), e.when_label));
        }
        md.push('\n');
    }
    if !open.is_empty() {
        md.push_str(&format!("### Open ({})\n", open.len()));
        for o in &open {
            md.push_str(&format!("- {}\n", md_escape(o)));
        }
        md.push('\n');
    }
    md.push_str(
        "---\nOpen items come from emails you asked me to watch and requests I couldn’t finish. \
         Your timetable and email priorities will appear here once they are set up.\n",
    );

    Some(Briefing { spoken, card_md: md, items: open.len() })
}

// ─── Glue ────────────────────────────────────────────────────────────

/// The last session's resume points, captured at startup BEFORE the recorder
/// writes anything for this session.
static BOOT_SNAPSHOT: Mutex<Option<Vec<ResumeEntry>>> = Mutex::new(None);

fn entries_from_store(dir: &std::path::Path, limit: usize) -> Vec<ResumeEntry> {
    super::with_store(dir, |s| {
        s.recent(Tier::Resume, limit)
            .into_iter()
            .filter_map(|h| {
                resume::decode(&h.value).map(|sample| ResumeEntry { sample, last_seen: h.last_seen })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Call once at startup, before `resume::start_recorder`.
pub fn snapshot_at_boot(app_data_dir: &std::path::Path) {
    let entries = entries_from_store(app_data_dir, 20);
    if let Ok(mut g) = BOOT_SNAPSHOT.lock() {
        *g = Some(entries);
    }
}

/// Important mail the inbox watcher filed in the last 3 days (Medium and up),
/// most urgent first, at most 5.
pub fn important_mail(dir: &std::path::Path, now_secs: i64) -> Vec<MailItem> {
    let mut rows: Vec<StoredMail> = super::with_store(dir, |s| {
        s.recent(Tier::Mail, 60)
            .into_iter()
            .filter_map(|h| serde_json::from_str::<StoredMail>(&h.value).ok())
            .collect()
    })
    .unwrap_or_default();
    rows.retain(|m| {
        now_secs - m.ts_ms / 1000 <= 3 * 24 * 3600
            && mailtriage::urgency_rank(mailtriage::urgency_from(&m.urgency)) >= mailtriage::urgency_rank(crate::google::types::AlertUrgency::Medium)
    });
    rows.sort_by(|a, b| {
        mailtriage::urgency_rank(mailtriage::urgency_from(&b.urgency))
            .cmp(&mailtriage::urgency_rank(mailtriage::urgency_from(&a.urgency)))
            .then(b.ts_ms.cmp(&a.ts_ms))
    });
    rows.into_iter()
        .take(5)
        .map(|m| MailItem { sender: m.sender, subject: m.subject, deadline: None, category: Some(m.category) })
        .collect()
}

fn gather<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    dir: &std::path::Path,
    resume: Vec<ResumeEntry>,
    events: Vec<AgendaItem>,
) -> Input {
    use chrono::Timelike;
    let now = chrono::Utc::now().timestamp();
    let mut mail: Vec<MailItem> = crate::memory::load_mail_watches(dir)
        .into_iter()
        .filter(|w| w.status == crate::google::types::WatchStatus::Active)
        .map(|w| MailItem { sender: w.sender, subject: w.subject, deadline: w.initial_deadline_raw, category: None })
        .collect();
    mail.extend(important_mail(dir, now));
    Input {
        now,
        hour: chrono::Local::now().hour(),
        name: crate::memory::read_user_profile(dir).and_then(|p| p.name),
        friend: crate::persona::is_friend(&crate::commands::read_persona_mode(app)),
        resume,
        mail,
        unfinished: crate::conversation::unfinished_requests(dir, now),
        events,
    }
}

/// Today's calendar for the briefing, best effort (never blocks it for long).
async fn today_events() -> Vec<AgendaItem> {
    match tokio::time::timeout(std::time::Duration::from_secs(5), super::google_io::agenda_for(0)).await {
        Ok(Ok(items)) => items,
        _ => vec![],
    }
}

/// Once-a-day spoken briefing shortly after startup, delivered through the
/// proactive policy (waits out meetings, speech and busy moments).
pub fn start_boot_briefing<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(BOOT_DELAY_SECS)).await;
        let Ok(dir) = app.path().app_data_dir() else { return };
        if !super::enabled(&dir) || !super::flag(&dir, "memcoreBriefing", true) {
            return;
        }
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let already = super::with_store(&dir, |s| s.meta_get("last_briefing_day").as_deref() == Some(today.as_str()))
            .unwrap_or(true);
        if already {
            return;
        }
        // Autostart runs at logon: do not talk to an empty room. Wait (up to
        // PRESENCE_WAIT_SECS) until there has been keyboard/mouse input lately.
        let mut waited = 0u64;
        while !user_present(resume::user_idle_secs()) {
            if waited >= PRESENCE_WAIT_SECS {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_secs(PRESENCE_POLL_SECS)).await;
            waited += PRESENCE_POLL_SECS;
        }
        let snapshot = BOOT_SNAPSHOT.lock().ok().and_then(|g| g.clone()).unwrap_or_default();
        let events = if super::flag(&dir, "memcoreMail", true) { today_events().await } else { vec![] };
        let input = gather(&app, &dir, snapshot, events);
        let Some(briefing) = build(&input) else { return };
        super::with_store(&dir, |s| s.meta_set("last_briefing_day", &today));
        crate::proactive_policy::submit(
            &app,
            "boot_briefing".to_string(),
            briefing.spoken,
            crate::google::types::AlertUrgency::Medium,
        );
    });
}

/// "Where did I leave off / show my briefing": built now from live data,
/// skipping the window the user is in at this moment.
pub async fn on_demand<R: tauri::Runtime>(app: &tauri::AppHandle<R>, dir: &std::path::Path) -> Option<Briefing> {
    let now = chrono::Utc::now().timestamp();
    let events = today_events().await;
    let resume: Vec<ResumeEntry> = entries_from_store(dir, 20)
        .into_iter()
        .filter(|r| now - r.last_seen > CURRENT_WINDOW_SECS)
        .collect();
    build(&gather(app, dir, resume, events))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(app: &str, title: &str, url: Option<&str>) -> Sample {
        Sample { app: app.into(), title: title.into(), url: url.map(String::from), activity: None }
    }

    fn input() -> Input {
        Input {
            now: 1_000_000,
            hour: 9,
            name: Some("Lakshya".into()),
            friend: false,
            resume: vec![],
            mail: vec![],
            unfinished: vec![],
            events: vec![],
        }
    }

    #[test]
    fn nothing_to_say_means_silence() {
        assert!(build(&input()).is_none());
        let mut i = input();
        i.resume = vec![ResumeEntry { sample: sample("code.exe", "a.rs", None), last_seen: i.now - MAX_RESUME_AGE_SECS - 1 }];
        assert!(build(&i).is_none(), "a resume point older than 7 days is not 'where you left off'");
    }

    #[test]
    fn says_where_you_left_off_with_a_real_age() {
        let mut i = input();
        i.resume = vec![ResumeEntry {
            sample: sample("brave.exe", "Two Sum", Some("https://leetcode.com/problems/two-sum")),
            last_seen: i.now - 3 * 3600,
        }];
        let b = build(&i).unwrap();
        assert!(b.spoken.starts_with("Good morning, sir."), "{}", b.spoken);
        assert!(b.spoken.contains("on LeetCode: “Two Sum”"), "{}", b.spoken);
        assert!(b.spoken.contains("about 3 hours ago"), "{}", b.spoken);
        assert_eq!(b.items, 0);
    }

    #[test]
    fn video_and_app_phrasing() {
        let v = sample("brave.exe", "Binary Search in 10 min - YouTube", Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert_eq!(describe(&v), "watching “Binary Search in 10 min” on YouTube");
        assert_eq!(describe(&sample("code.exe", "main.rs", None)), "in VS Code on “main.rs”");
        assert_eq!(
            describe(&sample("windowsterminal.exe", r"C:\WINDOWS\system32\cmd.exe ", None)),
            "in Terminal",
            "a shell path is noise, not a title"
        );
        assert_eq!(describe(&sample("brave.exe", "Healthiest nuts", Some("https://www.womenshealthmag.com/x"))), "reading “Healthiest nuts” in Brave");
    }

    #[test]
    fn open_items_are_counted_never_invented() {
        let mut i = input();
        i.mail = vec![MailItem { sender: "Prof. Rao".into(), subject: "Assignment 3".into(), deadline: Some("Friday 5 PM".into()), category: None }];
        i.unfinished = vec!["send the report to Asha".into()];
        let b = build(&i).unwrap();
        assert_eq!(b.items, 2);
        assert!(b.spoken.contains("2 things are open"), "{}", b.spoken);
        assert!(b.spoken.contains("deadline Friday 5 PM"), "{}", b.spoken);
        assert!(b.card_md.contains("### Open (2)") && b.card_md.contains("I couldn’t finish"), "{}", b.card_md);
        assert!(!b.spoken.contains('%'), "no made-up progress numbers: {}", b.spoken);
    }

    #[test]
    fn friend_mode_uses_the_first_name_and_late_hours_just_say_hello() {
        let mut i = input();
        i.friend = true;
        i.hour = 2;
        i.mail = vec![MailItem { sender: "GitHub".into(), subject: "Run failed".into(), deadline: None, category: None }];
        let b = build(&i).unwrap();
        assert!(b.spoken.starts_with("Hello, Lakshya."), "{}", b.spoken);
        i.name = None;
        assert!(build(&i).unwrap().spoken.starts_with("Hello."));
    }

    #[test]
    fn card_lists_up_to_three_recent_activities_and_escapes_markdown() {
        let mut i = input();
        i.resume = (0..5)
            .map(|n| ResumeEntry { sample: sample("code.exe", &format!("file{n}[x].rs"), None), last_seen: i.now - 600 * (n + 1) })
            .collect();
        let b = build(&i).unwrap();
        assert_eq!(b.card_md.matches("- in VS Code").count(), 3, "{}", b.card_md);
        assert!(b.card_md.contains("file0\\[x\\].rs"), "{}", b.card_md);
    }

    #[test]
    fn long_titles_are_clipped_for_speech() {
        let mut i = input();
        i.resume = vec![ResumeEntry { sample: sample("code.exe", &"x".repeat(300), None), last_seen: i.now - 600 }];
        assert!(build(&i).unwrap().spoken.chars().count() < 200);
    }

    #[test]
    fn triaged_mail_and_calendar_make_the_briefing() {
        let mut i = input();
        i.mail = vec![MailItem {
            sender: "Supabase".into(),
            subject: "Your project will be paused".into(),
            deadline: None,
            category: Some("database".into()),
        }];
        i.events = vec![
            AgendaItem { summary: "Dentist".into(), all_day: false, start_epoch: 0, end_epoch: 0, when_label: "3 PM".into() },
            AgendaItem { summary: "Team sync".into(), all_day: false, start_epoch: 0, end_epoch: 0, when_label: "5 PM".into() },
        ];
        let b = build(&i).unwrap();
        assert!(b.spoken.contains("You have 2 events today; the first is Dentist at 3 PM."), "{}", b.spoken);
        assert!(b.spoken.contains("a database service warning from Supabase"), "{}", b.spoken);
        assert!(b.card_md.contains("### Today's calendar") && b.card_md.contains("Team sync"), "{}", b.card_md);
        // A calendar alone is enough to say something.
        let mut only = input();
        only.events = i.events.clone();
        assert!(build(&only).is_some());
    }

    #[test]
    fn important_mail_is_recent_ranked_and_medium_plus() {
        use crate::google::types::AlertUrgency;
        use super::super::mailtriage::{to_stored, MailMeta, Triage};
        let dir = std::env::temp_dir().join(format!("nexus_brief_mail_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = chrono::Utc::now().timestamp();
        let mk = |id: &str, subject: &str, cat: Category, u: AlertUrgency, age_h: i64| {
            let m = MailMeta {
                id: id.into(),
                thread_id: "t".into(),
                from_name: "Sender".into(),
                from_email: "s@x.com".into(),
                subject: subject.into(),
                snippet: String::new(),
                labels: vec![],
                date_ms: (now - age_h * 3600) * 1000,
            };
            (Triage { category: cat, urgency: u, reason: "r".into() }, m)
        };
        super::super::with_store(&dir, |s| {
            for (t, m) in [
                mk("1", "Medium recent", Category::Github, AlertUrgency::Medium, 1),
                mk("2", "High older", Category::Exam, AlertUrgency::High, 5),
                mk("3", "Too old", Category::Exam, AlertUrgency::High, 24 * 5),
                mk("4", "Low", Category::Github, AlertUrgency::Low, 1),
            ] {
                s.observe(
                    &super::super::store::Observation {
                        tier: Tier::Mail,
                        key: format!("mail:{}", m.id),
                        value: serde_json::to_string(&to_stored(&t, &m, "me")).unwrap(),
                        source: "gmail:me".into(),
                        trust: super::super::store::Trust::Untrusted,
                        pinned: false,
                    },
                    now,
                )
                .unwrap();
            }
        });
        let got = important_mail(&dir, now);
        assert_eq!(got.iter().map(|m| m.subject.as_str()).collect::<Vec<_>>(), vec!["High older", "Medium recent"]);
        assert_eq!(got[0].category.as_deref(), Some("exam"));
    }

    #[test]
    fn presence_gate() {
        assert!(user_present(Some(5)));
        assert!(user_present(Some(59)));
        assert!(!user_present(Some(60)));
        assert!(!user_present(Some(3600)));
        assert!(user_present(None), "unknown idle time must not block the briefing forever");
    }

    #[test]
    fn ago_phrases() {
        assert_eq!(ago_phrase(1000, 990), "just now");
        assert_eq!(ago_phrase(10_000, 10_000 - 600), "about 10 minutes ago");
        assert_eq!(ago_phrase(100_000, 100_000 - 3600), "about an hour ago");
        assert_eq!(ago_phrase(200_000, 200_000 - 90_000), "yesterday");
        assert_eq!(ago_phrase(1_000_000, 1_000_000 - 3 * 86_400), "3 days ago");
    }
}
