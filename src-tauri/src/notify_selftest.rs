//! `nexus notify <scenario>`: run one notification scenario through the REAL pipeline of the running NEXUS, so the
//! Windows toast, the click animation, the CodeRabbit sidebar detail, grouping, recovery and expiry can be watched
//! without waiting for a real failure.
//!
//! The CLI starts a second launch of the exe with `--notify-test <name>`; the running instance receives it through
//! the single-instance callback (a launch argument cannot be sent from a web page, unlike a `nexus://` URL). Test
//! alerts use projects/repos named `selftest-<tag>` under `nexus-test`; every fresh scenario gets a new tag so it can
//! be repeated at will, while `still-failing` and `recovered` continue the most recent one. Nothing touches a provider
//! or credentials.

use tauri::{AppHandle, Runtime};

use crate::notify_brain::{Decision, Fact, Kind, Line, Obs};

pub const FLAG: &str = "--notify-test";
/// How long the `expired-soon` alert lives, so the owner can wait it out and click.
const SHORT_TTL_MS: u64 = 15_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    PrFailed,
    BuildFailed,
    StillFailing,
    Recovered,
    ExpiredSoon,
    SameName,
    Ambiguous,
    /// Simulates clicking the latest toast: expand animation, then the alert in the CodeRabbit sidebar.
    OpenLatest,
}

/// (command name, scenario, what the owner should see)
pub const SCENARIOS: [(&str, Scenario, &str); 8] = [
    ("pr-failed", Scenario::PrFailed, "spoken \"PR failed in selftest\" + a Windows notification; click it for the animation and the CodeRabbit sidebar"),
    ("build-failed", Scenario::BuildFailed, "spoken \"Build failed in selftest-build\" + a Windows notification"),
    ("still-failing", Scenario::StillFailing, "NO new notification or speech; the open alert then shows 2 failing runs"),
    ("recovered", Scenario::Recovered, "spoken \"selftest is green again\" + a Windows notification; detail shows Recovered"),
    ("expired-soon", Scenario::ExpiredSoon, "notification now; clicking it after 15 s shows \"This alert has expired.\""),
    ("same-name", Scenario::SameName, "two notifications alice/selftest and bob/selftest; each opens its own owner"),
    ("ambiguous", Scenario::Ambiguous, "spoken line only, no notification (a repo with several owners is not clickable)"),
    ("open", Scenario::OpenLatest, "no toast needed: plays the click animation and opens the latest alert in the sidebar"),
];

/// The scenario named by `--notify-test <name>`, or None for anything else (unknown names, no name, no flag).
pub fn parse_args(args: &[String]) -> Option<Scenario> {
    let i = args.iter().position(|a| a == FLAG)?;
    let name = args.get(i + 1)?;
    SCENARIOS.iter().find(|(n, _, _)| n == name).map(|(_, s, _)| *s)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub full_name: String,
    pub fact: Fact,
    /// Spoken line when the state machine is bypassed (`via_book == false`).
    pub line: Line,
    /// Run the observation through the real state machine (`notify_brain::observe_fact`).
    pub via_book: bool,
    /// Record in the ledger even when the state machine stays silent (what a "still failing" report does later).
    pub publish_when_silent: bool,
    /// Never create a card/ledger entry (ambiguous repo).
    pub publish: bool,
    pub speak: bool,
    pub ttl_ms: Option<u64>,
}

fn fact(project: &str, kind: Kind, obs: Obs, run: u64) -> Fact {
    Fact { service: "github", project: project.into(), kind, obs, run_ref: run.to_string() }
}

fn step(full_name: String, fact: Fact, line: Line) -> Step {
    Step { full_name, fact, line, via_book: true, publish_when_silent: false, publish: true, speak: true, ttl_ms: None }
}

fn failed_line(project: &str, kind: Kind, count: u32) -> Line {
    Line::Failed { project: project.into(), kind, count }
}

/// Scenarios that start a new alert (they get a fresh tag); the others continue the most recent PR alert.
pub fn starts_fresh(s: Scenario) -> bool {
    !matches!(s, Scenario::StillFailing | Scenario::Recovered | Scenario::OpenLatest)
}

/// The tag of a fresh scenario: unique per invocation, short, and valid inside a repository name.
pub fn fresh_tag(seed: u64) -> String {
    format!("{}", seed % 1_000_000)
}

/// Steps of a scenario. `seed` grows with every invocation so each run id differs from the last; `tag` names the
/// test project/repo (`selftest-<tag>`), so a repeated scenario is a NEW alert instead of a silent duplicate.
pub fn steps(s: Scenario, seed: u64, tag: &str) -> Vec<Step> {
    let pr = format!("selftest-{tag}");
    let build = format!("selftest-build-{tag}");
    let repo = |name: &str| format!("nexus-test/{name}");
    match s {
        Scenario::PrFailed => vec![step(repo(&pr), fact(&pr, Kind::Pr, Obs::Failed, seed), failed_line(&pr, Kind::Pr, 1))],
        Scenario::BuildFailed => vec![step(repo(&build), fact(&build, Kind::Build, Obs::Failed, seed), failed_line(&build, Kind::Build, 1))],
        Scenario::StillFailing => vec![Step {
            publish_when_silent: true,
            ..step(repo(&pr), fact(&pr, Kind::Pr, Obs::Failed, seed), Line::StillFailing { project: pr.clone(), kind: Kind::Pr, count: 2 })
        }],
        Scenario::Recovered => vec![step(repo(&pr), fact(&pr, Kind::Pr, Obs::Fixed, seed), Line::Fixed { project: pr.clone(), kind: Kind::Pr })],
        Scenario::ExpiredSoon => {
            let ttl = format!("selftest-ttl-{tag}");
            vec![Step { ttl_ms: Some(SHORT_TTL_MS), ..step(repo(&ttl), fact(&ttl, Kind::Build, Obs::Failed, seed), failed_line(&ttl, Kind::Build, 1)) }]
        }
        Scenario::SameName => {
            let name = format!("selftest-names-{tag}");
            let f = fact(&name, Kind::Build, Obs::Failed, seed);
            let l = failed_line(&name, Kind::Build, 1);
            vec![
                Step { via_book: false, ..step(format!("alice/{name}"), f.clone(), l.clone()) },
                Step { via_book: false, speak: false, ..step(format!("bob/{name}"), f, l) },
            ]
        }
        Scenario::Ambiguous => {
            let multi = format!("selftest-multi-{tag}");
            vec![Step { publish: false, ..step(repo(&multi), fact(&multi, Kind::Build, Obs::Failed, seed), failed_line(&multi, Kind::Build, 1)) }]
        }
        Scenario::OpenLatest => Vec::new(),
    }
}

/// The tag of the most recent fresh scenario, so `still-failing` and `recovered` continue it.
fn last_tag() -> &'static std::sync::Mutex<Option<String>> {
    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();
    LAST.get_or_init(|| std::sync::Mutex::new(None))
}

/// Runs the scenario in the live app. Every step is logged with `[NOTIFY-TEST]`.
pub fn run<R: Runtime>(app: &AppHandle<R>, s: Scenario) {
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(1);
    tracing::info!("[NOTIFY-TEST] scenario {s:?} (run ids from {seed})");
    if s == Scenario::OpenLatest {
        match crate::notify_hub::latest_event_id() {
            Some(id) => {
                tracing::info!("[NOTIFY-TEST] simulating a click on the latest alert {id}");
                tauri::async_runtime::block_on(crate::notify_hub::activate(app.clone(), id));
            }
            None => tracing::info!("[NOTIFY-TEST] no alert yet: run 'nexus notify pr-failed' first"),
        }
        return;
    }
    let tag = if starts_fresh(s) {
        let tag = fresh_tag(seed);
        *last_tag().lock().unwrap_or_else(|e| e.into_inner()) = Some(tag.clone());
        tag
    } else {
        match last_tag().lock().unwrap_or_else(|e| e.into_inner()).clone() {
            Some(tag) => tag,
            None => {
                tracing::info!("[NOTIFY-TEST] nothing to continue: run 'nexus notify pr-failed' first");
                return;
            }
        }
    };
    for st in steps(s, seed, &tag) {
        let decision = if st.via_book {
            crate::notify_brain::observe_fact(app, &st.fact, false)
        } else {
            Decision::Say(st.line.clone())
        };
        let line = match decision {
            Decision::Say(l) => {
                tracing::info!("[NOTIFY-TEST] state machine says: {}", crate::notify_brain::render_for("github", &l, None));
                if st.speak {
                    crate::notify_brain::say(app, "github", l.clone());
                }
                Some(l)
            }
            Decision::Silent => {
                tracing::info!(
                    "[NOTIFY-TEST] state machine stays silent for {} (by design for a repeat; fresh scenarios use a new name each time)",
                    st.fact.project
                );
                st.publish_when_silent.then(|| st.line.clone())
            }
        };
        let Some(line) = line else { continue };
        if !st.publish {
            tracing::info!("[NOTIFY-TEST] not published: an ambiguous repo never gets a clickable card");
            continue;
        }
        crate::notify_hub::publish_github_with(app, &st.full_name, &st.fact, &line, st.ttl_ms);
        tracing::info!("[NOTIFY-TEST] published {} run {}", st.full_name, st.fact.run_ref);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify_brain::Book;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_the_exact_flag_with_a_known_name_selects_a_scenario() {
        for (name, scenario, _) in SCENARIOS {
            assert_eq!(parse_args(&args(&["nexus.exe", FLAG, name])), Some(scenario));
        }
        assert_eq!(parse_args(&args(&["nexus.exe", FLAG, "nope"])), None);
        assert_eq!(parse_args(&args(&["nexus.exe", FLAG])), None);
        assert_eq!(parse_args(&args(&["nexus.exe", "pr-failed"])), None);
        assert_eq!(parse_args(&args(&["nexus.exe", "--notify-testx", "pr-failed"])), None);
        assert_eq!(parse_args(&args(&["nexus.exe", FLAG, "PR-FAILED"])), None);
        assert_eq!(parse_args(&args(&["nexus://notify/test?x"])), None);
    }

    #[test]
    fn every_scenario_has_steps_with_fresh_run_ids_and_test_names_only() {
        for (_, s, _) in SCENARIOS.iter().filter(|(_, s, _)| *s != Scenario::OpenLatest) {
            let (s, a) = (*s, steps(*s, 100, "7"));
            let b = steps(s, 101, "7");
            assert!(!a.is_empty(), "{s:?}");
            for (x, y) in a.iter().zip(&b) {
                assert_ne!(x.fact.run_ref, y.fact.run_ref, "{s:?}");
                assert!(x.full_name.contains("/selftest-"), "test repos only: {}", x.full_name);
                assert!(x.fact.project.starts_with("selftest"), "test projects only: {}", x.fact.project);
            }
        }
    }

    #[test]
    fn the_cli_lists_exactly_the_scenarios_the_app_understands() {
        let cli = include_str!("../../nexus.mjs");
        let block = cli.split("const NOTIFY_SCENARIOS = {").nth(1).and_then(|r| r.split("};").next()).expect("NOTIFY_SCENARIOS block");
        for (name, _, what) in SCENARIOS {
            assert!(block.contains(&format!("\"{name}\":")), "nexus.mjs is missing {name}");
            assert!(block.contains(what.split(';').next().unwrap()), "description drifted for {name}");
        }
        assert_eq!(block.matches("\": ").count(), SCENARIOS.len(), "nexus.mjs lists a scenario the app does not know");
    }

    #[test]
    fn a_repeated_scenario_is_a_new_alert_but_recovery_continues_the_last_one() {
        let mut book = Book::from_records(vec![]);
        // Two pr-failed in a row (fresh tags): both are news, never the silent duplicate that confused the first live run.
        for (seed, now) in [(1u64, 1_000u64), (2, 2_000)] {
            let tag = fresh_tag(seed * 1_000_003);
            let st = &steps(Scenario::PrFailed, seed, &tag)[0];
            assert!(matches!(book.observe(&st.fact, now, false), Decision::Say(Line::Failed { .. })), "run {seed}");
        }
        assert!(starts_fresh(Scenario::PrFailed) && starts_fresh(Scenario::SameName));
        assert!(!starts_fresh(Scenario::StillFailing) && !starts_fresh(Scenario::Recovered) && !starts_fresh(Scenario::OpenLatest));
        // Continuing scenarios use the same project as the alert they continue.
        let a = &steps(Scenario::PrFailed, 5, "42")[0];
        let b = &steps(Scenario::Recovered, 6, "42")[0];
        assert_eq!((a.fact.project.as_str(), a.full_name.as_str()), (b.fact.project.as_str(), b.full_name.as_str()));
        assert_ne!(fresh_tag(1), fresh_tag(2));
    }

    #[test]
    fn scenario_flags_match_their_purpose() {
        assert!(steps(Scenario::Ambiguous, 1, "7")[0].publish == false);
        assert!(steps(Scenario::OpenLatest, 1, "7").is_empty(), "open only simulates the click");
        assert_eq!(steps(Scenario::ExpiredSoon, 1, "7")[0].ttl_ms, Some(15_000));
        assert!(steps(Scenario::StillFailing, 1, "7")[0].publish_when_silent);
        let same = steps(Scenario::SameName, 1, "7");
        assert_eq!(same.len(), 2);
        assert_ne!(same[0].full_name, same[1].full_name);
        assert!(same[0].speak && !same[1].speak, "one spoken line for two alerts");
        assert!(same.iter().all(|s| !s.via_book), "bypasses the legacy bare-name state machine on purpose");
    }

    /// The state-machine half of each scenario, run in the order the owner will run them.
    #[test]
    fn the_scenarios_behave_as_the_table_promises_when_run_in_order() {
        let mut book = Book::from_records(vec![]);
        let mut say = |s: Scenario, seed: u64, now: u64| -> Vec<Decision> {
            steps(s, seed, "7").iter().filter(|st| st.via_book).map(|st| book.observe(&st.fact, now, false)).collect()
        };
        let first = say(Scenario::PrFailed, 1, 1_000);
        assert!(matches!(first[0], Decision::Say(Line::Failed { .. })), "pr-failed speaks");
        let again = say(Scenario::PrFailed, 2, 2_000);
        assert_eq!(again[0], Decision::Silent, "a second failure is silent");
        let still = say(Scenario::StillFailing, 3, 3_000);
        assert_eq!(still[0], Decision::Silent, "still-failing does not speak or card");
        let fixed = say(Scenario::Recovered, 4, 4_000);
        assert!(matches!(fixed[0], Decision::Say(Line::Fixed { .. })), "recovery speaks once");
        let fixed_again = say(Scenario::Recovered, 5, 5_000);
        assert_eq!(fixed_again[0], Decision::Silent, "a second recovery is silent");
        let cycle = say(Scenario::PrFailed, 6, 6_000);
        assert!(matches!(cycle[0], Decision::Say(Line::Failed { .. })), "the cycle can be repeated");
    }

    #[test]
    fn the_ledger_half_groups_a_still_failing_report_and_updates_on_recovery() {
        use crate::notify_event::Recorded;
        let mut led = crate::notify_event::Ledger::new();
        let rec = |led: &mut crate::notify_event::Ledger, s: Scenario, seed: u64, line: Line| {
            let st = &steps(s, seed, "7")[0];
            let ev = crate::notify_hub::github_event(&st.full_name, &st.fact, &line, 1_000 + seed).unwrap();
            led.record(ev).unwrap()
        };
        let a = rec(&mut led, Scenario::PrFailed, 1, failed_line("selftest-7", Kind::Pr, 1));
        let Recorded::Created(id) = a else { panic!("{a:?}") };
        let b = rec(&mut led, Scenario::StillFailing, 2, Line::StillFailing { project: "selftest-7".into(), kind: Kind::Pr, count: 2 });
        assert_eq!(b, Recorded::Grouped(id.clone()));
        let c = rec(&mut led, Scenario::Recovered, 3, Line::Fixed { project: "selftest-7".into(), kind: Kind::Pr });
        assert_eq!(c, Recorded::Recovered(id.clone()));
        let e = led.get(&id).unwrap();
        assert_eq!((e.occurrences, e.original_source_ref.as_str(), e.source_ref.as_str()), (2, "1", "3"));
        // same-name: two owners, two distinct alerts
        let s = steps(Scenario::SameName, 9, "7");
        let ids: Vec<_> = s
            .iter()
            .map(|st| crate::notify_hub::github_event(&st.full_name, &st.fact, &st.line, 1_000).unwrap().event_id)
            .collect();
        assert_ne!(ids[0], ids[1]);
    }
}
