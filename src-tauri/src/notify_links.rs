//! Where a notification click goes: provider-supplied, validated hints plus an ordered ladder of ways to open the
//! exact item (notification toast, phase 1).
//!
//! Rules (docs/changes/171, reports/NEXUS notification toast design.md):
//! - a link is built from TYPED fields, never from notification text; provider-supplied page URLs (a job's `html_url`,
//!   Vercel's `inspectorUrl`, a calendar `htmlLink`) are only accepted for the provider's own https hosts;
//! - every candidate is validated again at open time by `command_executor::open_url`;
//! - the ladder always ends at something that exists (the repo, the project, the inbox), never at a guess that 404s
//!   silently when a better page is known.
//!
//! Pure: no I/O, no network, no Tauri.

use crate::notify_event::{Provider, Target};
use serde::{Deserialize, Serialize};

const MAX_URL: usize = 500;

/// Optional, validated extras a producer knows at alert time. All fields default to "unknown".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenHints {
    /// GitHub Actions job id (documentation only; the page URL below is what is opened).
    pub job_id: Option<u64>,
    /// First failed step number of that job (for the `#step:<n>:1` log anchor).
    pub failed_step: Option<u32>,
    /// A page URL the provider's own API returned for this item (job `html_url`, Vercel `inspectorUrl`, calendar
    /// `htmlLink`). Accepted only for that provider's hosts.
    pub html_url: Option<String>,
    /// PR review comment id (`#discussion_r<id>`).
    pub comment_id: Option<u64>,
    /// Gmail account email (`authuser`).
    pub gmail_account: Option<String>,
    /// RFC 822 `Message-ID` header of the mail (id-independent Gmail search).
    pub rfc822_id: Option<String>,
    /// International phone number, digits only.
    pub phone_digits: Option<String>,
    /// Google Meet join URL.
    pub meet_url: Option<String>,
    /// Working folder of a coding agent.
    pub agent_cwd: Option<String>,
}

/// One way to open an item. A ladder is tried in order until one works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAction {
    /// https or a registered app protocol (`whatsapp://`, `vscode://`).
    Url(String),
    /// Bring the window of a coding agent forward (matched by process and folder name), else open the folder.
    FocusAgent { cwd: String },
}

/// Percent-encode everything except RFC 3986 unreserved characters. Pure.
pub fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn no_bad_chars(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_URL && !s.chars().any(|c| c.is_control() || c.is_whitespace() || "\"'<>\\`".contains(c))
}

/// The host of an `https://` URL, lowercased, when it has no userinfo and a plain host[:443].
fn https_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = authority.strip_suffix(":443").unwrap_or(authority);
    if host.contains(':') || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

fn host_allowed(provider: Provider, host: &str) -> bool {
    let under = |root: &str| host == root || host.ends_with(&format!(".{root}"));
    match provider {
        Provider::GitHub => host == "github.com",
        Provider::Vercel => under("vercel.com"),
        Provider::Render => host == "dashboard.render.com",
        Provider::Netlify => host == "app.netlify.com",
        Provider::Supabase => host == "supabase.com" || host == "app.supabase.com",
        Provider::Calendar => host == "www.google.com" || host == "calendar.google.com",
        Provider::Gmail | Provider::WhatsApp | Provider::Agent => false,
    }
}

/// A provider-supplied page URL accepted for `provider`: https, the provider's own host, no odd characters.
pub fn provider_page_ok(provider: Provider, url: &str) -> bool {
    no_bad_chars(url) && https_host(url).is_some_and(|h| host_allowed(provider, &h))
}

fn meet_ok(url: &str) -> bool {
    no_bad_chars(url) && https_host(url).is_some_and(|h| h == "meet.google.com")
}

fn email_ok(s: &str) -> bool {
    s.len() <= 254
        && s.matches('@').count() == 1
        && !s.starts_with('@')
        && !s.ends_with('@')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "@._+-".contains(c))
}

fn rfc822_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && s.chars().all(|c| c.is_ascii_graphic() && !"\"'\\`".contains(c))
}

fn digits_ok(s: &str) -> bool {
    (6..=15).contains(&s.len()) && s.chars().all(|c| c.is_ascii_digit())
}

fn cwd_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 260 && !s.chars().any(|c| c.is_control())
}

/// Every hint a producer set must be well-formed for the provider; the event is refused otherwise.
pub fn validate_hints(provider: Provider, h: &OpenHints) -> Result<(), &'static str> {
    if h.job_id == Some(0) {
        return Err("job_id");
    }
    if h.failed_step == Some(0) {
        return Err("failed_step");
    }
    if h.comment_id == Some(0) {
        return Err("comment_id");
    }
    if let Some(u) = &h.html_url {
        if !provider_page_ok(provider, u) {
            return Err("html_url");
        }
    }
    if h.gmail_account.as_deref().is_some_and(|e| !email_ok(e)) {
        return Err("gmail_account");
    }
    if h.rfc822_id.as_deref().is_some_and(|m| !rfc822_ok(m)) {
        return Err("rfc822_id");
    }
    if h.phone_digits.as_deref().is_some_and(|p| !digits_ok(p)) {
        return Err("phone_digits");
    }
    if h.meet_url.as_deref().is_some_and(|m| !meet_ok(m)) {
        return Err("meet_url");
    }
    if h.agent_cwd.as_deref().is_some_and(|c| !cwd_ok(c)) {
        return Err("agent_cwd");
    }
    Ok(())
}

fn push_unique(v: &mut Vec<OpenAction>, a: OpenAction) {
    if !v.contains(&a) {
        v.push(a);
    }
}

fn url(v: &mut Vec<OpenAction>, u: String) {
    push_unique(v, OpenAction::Url(u));
}

fn gmail_base(account: &Option<String>) -> String {
    match account {
        Some(a) => format!("https://mail.google.com/mail/?authuser={}", enc(a)),
        None => "https://mail.google.com/mail/".to_string(),
    }
}

/// The ordered ways to open this item. Never empty for a supported target.
pub fn open_ladder(provider: Provider, target: &Target, h: &OpenHints) -> Vec<OpenAction> {
    let mut v: Vec<OpenAction> = Vec::new();
    match target {
        Target::GitHub { host, owner, repo, pr, run_id, .. } if host == "github.com" => {
            let base = format!("https://github.com/{owner}/{repo}");
            if let (Some(p), Some(c)) = (pr, h.comment_id) {
                url(&mut v, format!("{base}/pull/{p}#discussion_r{c}"));
            }
            if let Some(page) = h.html_url.as_deref().filter(|u| provider_page_ok(Provider::GitHub, u)) {
                // The step anchor only makes sense on a job page, and only when the page has no fragment yet.
                if page.contains("/job/") && !page.contains('#') {
                    if let Some(n) = h.failed_step {
                        url(&mut v, format!("{page}#step:{n}:1"));
                    }
                }
                url(&mut v, page.to_string());
            }
            // Job logs were asked for (a job page is known): run, then PR. Otherwise the PR first, as before.
            let job_known = h.html_url.is_some();
            let run_url = run_id.map(|r| format!("{base}/actions/runs/{r}"));
            let pr_url = pr.map(|p| format!("{base}/pull/{p}"));
            let (first, second) = if job_known { (run_url, pr_url) } else { (pr_url, run_url) };
            for u in [first, second].into_iter().flatten() {
                url(&mut v, u);
            }
            url(&mut v, base);
        }
        Target::Repo { host, owner, repo } if host == "github.com" => {
            url(&mut v, format!("https://github.com/{owner}/{repo}"));
        }
        Target::Gmail { thread_id, message_id } => {
            let base = gmail_base(&h.gmail_account);
            if let Some(m) = h.rfc822_id.as_deref() {
                url(&mut v, format!("{base}#search/{}", enc(&format!("rfc822msgid:{m}"))));
            }
            if let Some(m) = message_id {
                url(&mut v, format!("{base}#all/{m}"));
            }
            url(&mut v, format!("{base}#inbox/{thread_id}"));
            url(&mut v, format!("{base}#inbox"));
        }
        Target::Deployment { provider: p, team, project, deployment_id, .. } => {
            if let Some(page) = h.html_url.as_deref().filter(|u| provider_page_ok(*p, u)) {
                url(&mut v, page.to_string());
            }
            match p {
                Provider::Vercel => {
                    url(&mut v, format!("https://vercel.com/{team}/{project}/{deployment_id}"));
                    url(&mut v, format!("https://vercel.com/{team}/{project}"));
                }
                Provider::Render => {
                    // `project` holds the service id (srv-...); the deploy path is unverified, the service page is not.
                    if project.starts_with("srv-") {
                        url(&mut v, format!("https://dashboard.render.com/web/{project}/deploys/{deployment_id}"));
                        url(&mut v, format!("https://dashboard.render.com/web/{project}"));
                    }
                    url(&mut v, "https://dashboard.render.com/".to_string());
                }
                Provider::Netlify => {
                    url(&mut v, format!("https://app.netlify.com/projects/{project}/deploys/{deployment_id}"));
                    url(&mut v, format!("https://app.netlify.com/projects/{project}"));
                }
                Provider::Supabase => {
                    url(&mut v, format!("https://supabase.com/dashboard/project/{project}"));
                }
                Provider::GitHub | Provider::Gmail | Provider::WhatsApp | Provider::Calendar | Provider::Agent => {}
            }
        }
        Target::WhatsApp { .. } => {
            if let Some(d) = h.phone_digits.as_deref() {
                url(&mut v, format!("whatsapp://send?phone={d}"));
                url(&mut v, format!("https://wa.me/{d}"));
            }
        }
        Target::Calendar { .. } => {
            if let Some(page) = h.html_url.as_deref().filter(|u| provider_page_ok(Provider::Calendar, u)) {
                url(&mut v, page.to_string());
            }
            if let Some(m) = h.meet_url.as_deref().filter(|m| meet_ok(m)) {
                url(&mut v, m.to_string());
            }
            url(&mut v, "https://calendar.google.com/calendar/".to_string());
        }
        Target::Agent { .. } => {
            if let Some(c) = h.agent_cwd.as_deref().filter(|c| cwd_ok(c)) {
                push_unique(&mut v, OpenAction::FocusAgent { cwd: c.to_string() });
            }
        }
        _ => {}
    }
    let _ = provider;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(pr: Option<u64>, run: Option<u64>) -> Target {
        Target::GitHub { host: "github.com".into(), owner: "o".into(), repo: "r".into(), pr, run_id: run, sha: None }
    }

    fn urls(v: &[OpenAction]) -> Vec<String> {
        v.iter()
            .filter_map(|a| if let OpenAction::Url(u) = a { Some(u.clone()) } else { None })
            .collect()
    }

    #[test]
    fn a_failed_job_opens_at_its_failing_step_then_degrades() {
        let h = OpenHints {
            job_id: Some(99),
            failed_step: Some(5),
            html_url: Some("https://github.com/o/r/actions/runs/7/job/99".into()),
            ..Default::default()
        };
        let u = urls(&open_ladder(Provider::GitHub, &gh(None, Some(7)), &h));
        assert_eq!(u[0], "https://github.com/o/r/actions/runs/7/job/99#step:5:1");
        assert_eq!(u[1], "https://github.com/o/r/actions/runs/7/job/99");
        assert_eq!(u[2], "https://github.com/o/r/actions/runs/7");
        assert_eq!(u.last().unwrap(), "https://github.com/o/r");
    }

    #[test]
    fn a_review_comment_opens_on_its_anchor_first() {
        let h = OpenHints { comment_id: Some(1234), ..Default::default() };
        let u = urls(&open_ladder(Provider::GitHub, &gh(Some(4), None), &h));
        assert_eq!(u[0], "https://github.com/o/r/pull/4#discussion_r1234");
        assert_eq!(u[1], "https://github.com/o/r/pull/4");
    }

    #[test]
    fn a_provider_url_from_another_host_is_never_used() {
        let evil = OpenHints { html_url: Some("https://evil.example/o/r/actions/runs/7/job/9".into()), failed_step: Some(1), ..Default::default() };
        assert_eq!(validate_hints(Provider::GitHub, &evil), Err("html_url"));
        let u = urls(&open_ladder(Provider::GitHub, &gh(None, Some(7)), &evil));
        assert!(u.iter().all(|x| x.starts_with("https://github.com/")), "{u:?}");
        for bad in [
            "http://github.com/o/r",
            "https://github.com@evil.example/o",
            "https://github.com.evil.example/o",
            "https://github.com:8080/o",
            "https://github.com/o r",
            "javascript:alert(1)",
        ] {
            assert!(!provider_page_ok(Provider::GitHub, bad), "{bad}");
        }
        assert!(provider_page_ok(Provider::Vercel, "https://vercel.com/team/proj/abc"));
        assert!(provider_page_ok(Provider::Vercel, "https://my-team.vercel.com/x"));
        assert!(!provider_page_ok(Provider::Vercel, "https://vercel.com.evil.example/x"));
    }

    #[test]
    fn gmail_ladder_prefers_the_message_id_search_then_ids_then_the_inbox() {
        let t = Target::Gmail { thread_id: "18c2f".into(), message_id: Some("18c2e".into()) };
        let h = OpenHints { gmail_account: Some("me@gmail.com".into()), rfc822_id: Some("<CAF+abc@mail.gmail.com>".into()), ..Default::default() };
        assert_eq!(validate_hints(Provider::Gmail, &h), Ok(()));
        let u = urls(&open_ladder(Provider::Gmail, &t, &h));
        assert_eq!(u[0], "https://mail.google.com/mail/?authuser=me%40gmail.com#search/rfc822msgid%3A%3CCAF%2Babc%40mail.gmail.com%3E");
        assert_eq!(u[1], "https://mail.google.com/mail/?authuser=me%40gmail.com#all/18c2e");
        assert_eq!(u[2], "https://mail.google.com/mail/?authuser=me%40gmail.com#inbox/18c2f");
        assert_eq!(u[3], "https://mail.google.com/mail/?authuser=me%40gmail.com#inbox");
    }

    #[test]
    fn gmail_without_an_account_still_opens_the_thread() {
        let t = Target::Gmail { thread_id: "t1".into(), message_id: None };
        let u = urls(&open_ladder(Provider::Gmail, &t, &OpenHints::default()));
        assert_eq!(u[0], "https://mail.google.com/mail/#inbox/t1");
    }

    #[test]
    fn malformed_hints_are_refused() {
        for (h, field) in [
            (OpenHints { gmail_account: Some("not an email".into()), ..Default::default() }, "gmail_account"),
            (OpenHints { rfc822_id: Some("a b".into()), ..Default::default() }, "rfc822_id"),
            (OpenHints { phone_digits: Some("+91 98".into()), ..Default::default() }, "phone_digits"),
            (OpenHints { meet_url: Some("https://evil.example/m".into()), ..Default::default() }, "meet_url"),
            (OpenHints { job_id: Some(0), ..Default::default() }, "job_id"),
        ] {
            assert_eq!(validate_hints(Provider::Gmail, &h), Err(field));
        }
    }

    #[test]
    fn deployments_use_the_provider_url_when_known_else_the_best_constructed_page() {
        let vercel = Target::Deployment { provider: Provider::Vercel, team: "team".into(), project: "zync".into(), deployment_id: "dpl_1".into(), revision: None };
        let h = OpenHints { html_url: Some("https://vercel.com/team/zync/AbC123".into()), ..Default::default() };
        let u = urls(&open_ladder(Provider::Vercel, &vercel, &h));
        assert_eq!(u[0], "https://vercel.com/team/zync/AbC123");
        assert_eq!(u.last().unwrap(), "https://vercel.com/team/zync");
        let render = Target::Deployment { provider: Provider::Render, team: "me".into(), project: "srv-abc".into(), deployment_id: "dep-1".into(), revision: None };
        let u = urls(&open_ladder(Provider::Render, &render, &OpenHints::default()));
        assert_eq!(u[0], "https://dashboard.render.com/web/srv-abc/deploys/dep-1");
        assert_eq!(u[1], "https://dashboard.render.com/web/srv-abc");
        let netlify = Target::Deployment { provider: Provider::Netlify, team: "me".into(), project: "my-site".into(), deployment_id: "d1".into(), revision: None };
        let u = urls(&open_ladder(Provider::Netlify, &netlify, &OpenHints::default()));
        assert_eq!(u[0], "https://app.netlify.com/projects/my-site/deploys/d1");
        assert_eq!(u[1], "https://app.netlify.com/projects/my-site");
        let sb = Target::Deployment { provider: Provider::Supabase, team: "me".into(), project: "abcdefgh".into(), deployment_id: "x".into(), revision: None };
        assert_eq!(urls(&open_ladder(Provider::Supabase, &sb, &OpenHints::default())), vec!["https://supabase.com/dashboard/project/abcdefgh".to_string()]);
    }

    #[test]
    fn whatsapp_opens_the_chat_app_then_the_browser_link() {
        let t = Target::WhatsApp { chat: "c1".into() };
        let h = OpenHints { phone_digits: Some("919876543210".into()), ..Default::default() };
        let u = urls(&open_ladder(Provider::WhatsApp, &t, &h));
        assert_eq!(u, vec!["whatsapp://send?phone=919876543210".to_string(), "https://wa.me/919876543210".to_string()]);
        assert!(open_ladder(Provider::WhatsApp, &t, &OpenHints::default()).is_empty(), "no number, no guess");
    }

    #[test]
    fn calendar_and_agent_ladders() {
        let c = Target::Calendar { event_id: "e1".into() };
        let h = OpenHints {
            html_url: Some("https://www.google.com/calendar/event?eid=abc".into()),
            meet_url: Some("https://meet.google.com/abc-defg-hij".into()),
            ..Default::default()
        };
        let u = urls(&open_ladder(Provider::Calendar, &c, &h));
        assert_eq!(u[0], "https://www.google.com/calendar/event?eid=abc");
        assert_eq!(u[1], "https://meet.google.com/abc-defg-hij");
        assert_eq!(u.last().unwrap(), "https://calendar.google.com/calendar/");
        let a = Target::Agent { agent: "claude".into(), project: "zync".into() };
        let h = OpenHints { agent_cwd: Some("D:\\ULTRON".into()), ..Default::default() };
        assert_eq!(open_ladder(Provider::Agent, &a, &h), vec![OpenAction::FocusAgent { cwd: "D:\\ULTRON".into() }]);
    }

    #[test]
    fn a_step_anchor_is_added_only_to_a_job_page() {
        let h = OpenHints { failed_step: Some(3), html_url: Some("https://github.com/o/r/pull/5".into()), ..Default::default() };
        let u = urls(&open_ladder(Provider::GitHub, &gh(None, None), &h));
        assert_eq!(u[0], "https://github.com/o/r/pull/5");
        assert!(!u.iter().any(|x| x.contains("#step")));
    }
}
