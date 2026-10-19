//! Following several coding agents at once (doc 146, phase P2): the pure parts.
//!
//! `watch_runner` looks at each agent every few seconds (hook events, process tree, the cross-check model) and hands
//! this module one `TrackSnapshot` per agent. From those it works out, with no clock of its own and no I/O:
//!   * `aggregate`: is EVERY tracked agent done, what is the worst verdict among them, who is still working;
//!   * `Announcer`: which agents have settled as finished since the last look, each told exactly once (and told
//!     again if the agent starts a new turn and finishes again);
//!   * `Joiner`: which agents count as part of the plan yet ("all my agents" only follows agents that are doing
//!     something; a named agent that never starts is dropped after a while instead of being waited for forever);
//!   * the sentences NEXUS says ("Claude Code finished clean. Codex is still working.").
//!
//! Nothing here can power anything off or touch a window; it only decides what is true and what to say.

use crate::session_verdict::{spoken_reasons, Verdict};
use std::collections::{BTreeMap, BTreeSet};

/// An agent must stay finished this long before it is announced (a stop followed by a quick new turn is not "done").
pub const ANNOUNCE_SETTLE_MS: u64 = 15_000;
/// A named agent (in a list) that shows no activity for this long is not running, so it is not waited for.
pub const PENDING_DROP_MS: u64 = 90_000;
/// With "all my agents": how long with nothing working before NEXUS says so (once).
pub const NOTHING_RUNNING_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub label: String,
    pub verdict: Verdict,
}

/// What the runner saw of one agent this tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackSnapshot {
    /// machine name ("claude")
    pub name: String,
    /// spoken name ("Claude Code")
    pub label: String,
    /// it is part of the plan (it showed activity, or the plan names exactly this one agent)
    pub joined: bool,
    /// two independent signs that it ended and nothing says it is still busy (`session_verdict::has_ended`)
    pub ended: bool,
    pub verdict: Verdict,
    /// the cross-check model overrode a failure word for this agent
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Aggregate {
    pub tracked: usize,
    /// at least one agent is tracked and every tracked agent has ended
    pub ended: bool,
    /// the worst verdict among the tracked agents that have ended (Hold beats Unsure beats Go)
    pub verdict: Verdict,
    pub note: Option<String>,
    /// agents that have ended, in the order they are listed
    pub outcomes: Vec<Outcome>,
    /// labels of the agents still working
    pub working: Vec<String>,
}

pub fn aggregate(snaps: &[TrackSnapshot]) -> Aggregate {
    let joined: Vec<&TrackSnapshot> = snaps.iter().filter(|s| s.joined).collect();
    if joined.is_empty() {
        return Aggregate { tracked: 0, ended: false, verdict: Verdict::Unsure(Vec::new()), note: None, outcomes: Vec::new(), working: Vec::new() };
    }
    let ended = joined.iter().all(|s| s.ended);
    let mut hold = Vec::new();
    let mut unsure = Vec::new();
    for s in joined.iter().filter(|s| s.ended) {
        match &s.verdict {
            Verdict::Go => {}
            Verdict::Hold(r) => hold.extend(r.iter().cloned()),
            Verdict::Unsure(r) => unsure.extend(r.iter().cloned()),
        }
    }
    let any_hold = joined.iter().any(|s| s.ended && matches!(s.verdict, Verdict::Hold(_)));
    let any_unsure = joined.iter().any(|s| s.ended && matches!(s.verdict, Verdict::Unsure(_)));
    let verdict = if any_hold {
        Verdict::Hold(hold)
    } else if any_unsure || !ended {
        Verdict::Unsure(unsure)
    } else {
        Verdict::Go
    };
    Aggregate {
        tracked: joined.len(),
        ended,
        verdict,
        note: joined.iter().find_map(|s| s.note.clone()),
        outcomes: joined.iter().filter(|s| s.ended).map(|s| Outcome { label: s.label.clone(), verdict: s.verdict.clone() }).collect(),
        working: joined.iter().filter(|s| !s.ended).map(|s| s.label.clone()).collect(),
    }
}

// ─── What gets said ──────────────────────────────────────────────────

/// "Claude Code finished clean" / "Antigravity stopped, but the output mentions a failure (3 failing)" /
/// "Cursor may be done, but I could not read its final output".
pub fn outcome_line(o: &Outcome) -> String {
    match &o.verdict {
        Verdict::Go => format!("{} finished clean", o.label),
        Verdict::Hold(_) => format!("{} stopped, but {}", o.label, spoken_reasons(&o.verdict)),
        Verdict::Unsure(_) => format!("{} may be done, but {}", o.label, spoken_reasons(&o.verdict)),
    }
}

fn join_labels(labels: &[String]) -> String {
    match labels {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {}", rest.join(", "), last),
    }
}

/// While others are still working: "Claude Code finished clean. Codex and Cursor are still working."
pub fn progress_line(finished: &[Outcome], working: &[String]) -> String {
    let done: Vec<String> = finished.iter().map(outcome_line).collect();
    let mut s = done.join(". ");
    if !working.is_empty() {
        let verb = if working.len() == 1 { "is" } else { "are" };
        s.push_str(&format!(". {} {verb} still working", join_labels(working)));
    }
    s.push('.');
    s
}

/// When the last one is done.
pub fn final_summary(outcomes: &[Outcome]) -> String {
    let all_clean = outcomes.iter().all(|o| o.verdict == Verdict::Go);
    match outcomes {
        [] => "Nothing was being tracked, sir.".to_string(),
        [one] if all_clean => format!("{} is done, sir, and it finished clean.", one.label),
        [one] => format!("{}, sir.", outcome_line(one)),
        many if all_clean => format!("All {} of your agents are done, sir, and every one finished clean.", many.len()),
        many => format!(
            "All {} of your agents are done, sir. {}.",
            many.len(),
            many.iter().map(outcome_line).collect::<Vec<_>>().join(". ")
        ),
    }
}

// ─── Telling each agent's finish once ────────────────────────────────

#[derive(Debug, Default)]
pub struct Announcer {
    first_ended_ms: BTreeMap<String, u64>,
    told: BTreeSet<String>,
}

impl Announcer {
    /// Agents that have now been finished for `ANNOUNCE_SETTLE_MS` and were not announced yet. An agent that starts
    /// working again is forgotten, so its next finish is announced too.
    pub fn step(&mut self, now_ms: u64, snaps: &[TrackSnapshot]) -> Vec<Outcome> {
        let mut out = Vec::new();
        for s in snaps.iter().filter(|s| s.joined) {
            if s.ended {
                let first = *self.first_ended_ms.entry(s.name.clone()).or_insert(now_ms);
                if now_ms.saturating_sub(first) >= ANNOUNCE_SETTLE_MS && self.told.insert(s.name.clone()) {
                    out.push(Outcome { label: s.label.clone(), verdict: s.verdict.clone() });
                }
            } else {
                self.first_ended_ms.remove(&s.name);
                self.told.remove(&s.name);
            }
        }
        out
    }
}

// ─── Who counts as part of the plan ──────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinMode {
    /// One named agent (or the one in front): tracked from the start, however quiet it is.
    Single,
    /// "claude and codex": each joins when it shows activity; one that never does is dropped after `PENDING_DROP_MS`.
    ExplicitMulti,
    /// "all my agents": any agent joins when it shows activity; nothing is waited for before that.
    All,
}

#[derive(Debug, Default, PartialEq)]
pub struct JoinUpdate {
    pub joined: Vec<String>,
    pub dropped: Vec<String>,
}

#[derive(Debug)]
pub struct Joiner {
    mode: JoinMode,
    started_ms: u64,
    joined: BTreeSet<String>,
    dropped: BTreeSet<String>,
}

impl Joiner {
    pub fn new(mode: JoinMode, started_ms: u64) -> Joiner {
        Joiner { mode, started_ms, joined: BTreeSet::new(), dropped: BTreeSet::new() }
    }

    #[cfg(test)]
    pub fn is_joined(&self, name: &str) -> bool {
        self.joined.contains(name)
    }

    /// `candidates` = (machine name, shows activity now or since the plan began).
    pub fn step(&mut self, now_ms: u64, candidates: &[(String, bool)]) -> JoinUpdate {
        let mut up = JoinUpdate::default();
        for (name, active) in candidates {
            let wants = *active || self.mode == JoinMode::Single;
            if wants && !self.joined.contains(name) {
                self.joined.insert(name.clone());
                self.dropped.remove(name);
                up.joined.push(name.clone());
            } else if !wants
                && self.mode == JoinMode::ExplicitMulti
                && !self.joined.contains(name)
                && now_ms.saturating_sub(self.started_ms) >= PENDING_DROP_MS
                && self.dropped.insert(name.clone())
            {
                up.dropped.push(name.clone());
            }
        }
        up
    }

    /// Nothing is joined: the plan has nothing to follow (yet).
    pub fn idle(&self) -> bool {
        self.joined.is_empty()
    }

    /// How long the plan has existed.
    pub fn age_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.started_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_verdict::Reason;

    fn snap(name: &str, label: &str, joined: bool, ended: bool, verdict: Verdict) -> TrackSnapshot {
        TrackSnapshot { name: name.into(), label: label.into(), joined, ended, verdict, note: None }
    }

    fn fail() -> Verdict {
        Verdict::Hold(vec![Reason::FailureMarker("3 failing".into(), "Fix 3 failing tests".into())])
    }

    #[test]
    fn it_is_done_only_when_every_tracked_agent_is_and_the_worst_verdict_wins() {
        let none = aggregate(&[]);
        assert!(!none.ended && none.tracked == 0);
        // agents that never joined do not count either way
        let only_pending = aggregate(&[snap("codex", "Codex", false, false, Verdict::Go)]);
        assert!(!only_pending.ended && only_pending.tracked == 0);

        let mixed = aggregate(&[snap("claude", "Claude Code", true, true, Verdict::Go), snap("codex", "Codex", true, false, Verdict::Unsure(vec![]))]);
        assert!(!mixed.ended, "Codex is still working");
        assert_eq!(mixed.working, vec!["Codex".to_string()]);
        assert_eq!(mixed.outcomes.len(), 1);

        let all_clean = aggregate(&[snap("claude", "Claude Code", true, true, Verdict::Go), snap("codex", "Codex", true, true, Verdict::Go)]);
        assert!(all_clean.ended && all_clean.verdict == Verdict::Go && all_clean.working.is_empty());

        let worst = aggregate(&[
            snap("claude", "Claude Code", true, true, Verdict::Go),
            snap("agy", "Antigravity", true, true, fail()),
            snap("cursor", "Cursor", true, true, Verdict::Unsure(vec![Reason::NothingToJudge])),
        ]);
        assert!(worst.ended);
        assert!(matches!(&worst.verdict, Verdict::Hold(r) if r.len() == 1), "a hold beats an unsure: {:?}", worst.verdict);
        let unsure = aggregate(&[snap("claude", "Claude Code", true, true, Verdict::Go), snap("cursor", "Cursor", true, true, Verdict::Unsure(vec![Reason::NothingToJudge]))]);
        assert!(matches!(unsure.verdict, Verdict::Unsure(_)));
        // the cross-check note of any agent is carried along
        let mut with_note = snap("agy", "Antigravity", true, true, Verdict::Go);
        with_note.note = Some("Its report mentions \"3 failing\", which I read as a finding, not a failure.".into());
        assert!(aggregate(&[snap("claude", "Claude Code", true, true, Verdict::Go), with_note]).note.is_some());
    }

    #[test]
    fn each_finish_is_told_once_after_it_settles_and_again_after_a_new_turn() {
        let mut a = Announcer::default();
        let done = |ended| vec![snap("claude", "Claude Code", true, ended, Verdict::Go), snap("codex", "Codex", true, false, Verdict::Unsure(vec![]))];
        assert!(a.step(0, &done(false)).is_empty());
        assert!(a.step(3_000, &done(true)).is_empty(), "just stopped: not settled yet");
        assert!(a.step(3_000 + ANNOUNCE_SETTLE_MS - 1, &done(true)).is_empty());
        let told = a.step(3_000 + ANNOUNCE_SETTLE_MS, &done(true));
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].label, "Claude Code");
        assert!(a.step(60_000, &done(true)).is_empty(), "never twice");
        // a quick new turn, then finished again: told again
        assert!(a.step(63_000, &done(false)).is_empty());
        assert!(a.step(66_000, &done(true)).is_empty());
        assert_eq!(a.step(66_000 + ANNOUNCE_SETTLE_MS, &done(true)).len(), 1);
        // a flicker (stopped for one tick) is never told
        let mut b = Announcer::default();
        b.step(0, &done(true));
        b.step(3_000, &done(false));
        assert!(b.step(3_000 + ANNOUNCE_SETTLE_MS, &done(true)).is_empty());
    }

    #[test]
    fn the_sentences_say_who_finished_how_and_who_is_still_working() {
        let clean = Outcome { label: "Claude Code".into(), verdict: Verdict::Go };
        let bad = Outcome { label: "Antigravity".into(), verdict: fail() };
        let vague = Outcome { label: "Cursor".into(), verdict: Verdict::Unsure(vec![Reason::NothingToJudge]) };
        assert_eq!(outcome_line(&clean), "Claude Code finished clean");
        assert_eq!(outcome_line(&bad), "Antigravity stopped, but the output mentions a failure (3 failing)");
        assert_eq!(outcome_line(&vague), "Cursor may be done, but I could not read its final output");
        assert_eq!(progress_line(&[clean.clone()], &["Codex".into()]), "Claude Code finished clean. Codex is still working.");
        assert_eq!(
            progress_line(&[clean.clone(), bad.clone()], &["Codex".into(), "Cursor".into(), "Aider".into()]),
            "Claude Code finished clean. Antigravity stopped, but the output mentions a failure (3 failing). Codex, Cursor and Aider are still working."
        );
        assert_eq!(final_summary(&[clean.clone()]), "Claude Code is done, sir, and it finished clean.");
        assert_eq!(final_summary(&[clean.clone(), clean.clone(), clean.clone()]), "All 3 of your agents are done, sir, and every one finished clean.");
        let s = final_summary(&[clean.clone(), bad.clone()]);
        assert!(s.starts_with("All 2 of your agents are done, sir.") && s.contains("Claude Code finished clean") && s.contains("Antigravity stopped, but"), "{s}");
        assert!(final_summary(&[bad.clone()]).starts_with("Antigravity stopped, but") && final_summary(&[bad.clone()]).ends_with(", sir."));
        assert_eq!(final_summary(&[]), "Nothing was being tracked, sir.");
    }

    #[test]
    fn a_single_agent_is_tracked_from_the_start_a_list_waits_90_s_and_all_only_follows_activity() {
        let cands = |claude, codex| vec![("claude".to_string(), claude), ("codex".to_string(), codex)];
        // single: joined at once even when quiet
        let mut j = Joiner::new(JoinMode::Single, 0);
        assert_eq!(j.step(0, &[("claude".to_string(), false)]).joined, vec!["claude".to_string()]);
        // explicit list: Codex never shows activity
        let mut j = Joiner::new(JoinMode::ExplicitMulti, 0);
        assert!(j.step(0, &cands(true, false)).joined == vec!["claude".to_string()]);
        assert!(j.step(PENDING_DROP_MS - 1, &cands(true, false)).dropped.is_empty());
        assert_eq!(j.step(PENDING_DROP_MS, &cands(true, false)).dropped, vec!["codex".to_string()]);
        assert!(j.step(PENDING_DROP_MS + 3_000, &cands(true, false)).dropped.is_empty(), "dropped once");
        // ...and joins after all if it starts later
        assert_eq!(j.step(PENDING_DROP_MS + 6_000, &cands(true, true)).joined, vec!["codex".to_string()]);
        // all: nothing is waited for, nothing is dropped
        let mut j = Joiner::new(JoinMode::All, 0);
        let up = j.step(10 * PENDING_DROP_MS, &cands(false, false));
        assert!(up == JoinUpdate::default() && j.idle());
        assert_eq!(j.step(10 * PENDING_DROP_MS + 3_000, &cands(false, true)).joined, vec!["codex".to_string()]);
        assert!(j.is_joined("codex") && !j.is_joined("claude") && !j.idle());
        assert_eq!(j.age_ms(10 * PENDING_DROP_MS + 3_000), 10 * PENDING_DROP_MS + 3_000);
    }
}
