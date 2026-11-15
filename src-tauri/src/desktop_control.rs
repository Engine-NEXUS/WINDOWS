//! Native desktop decision rules. No OS, model, input, credential or application I/O.
use serde::{Deserialize, Serialize};

pub const VERIFY_MS: u64 = 2_000;
pub const FOCUS_MS: u64 = 5_000;
pub const POLL_MS: u64 = 50;
pub const CHOICE_MS: u64 = 30_000;
pub fn expected_executable(name: &str) -> Option<&'static str> {
    match normalize(name).as_str() {
        "edge" | "microsoft edge" | "msedge" => Some("msedge.exe"),
        "explorer" | "file explorer" => Some("explorer.exe"),
        "notepad" => Some("notepad.exe"),
        "settings" | "windows settings" => Some("systemsettings.exe"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowIdentity {
    pub hwnd: isize,
    pub pid: u32,
    pub created: u64,
    pub exe: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetQuery {
    pub label: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub learned_alias: Option<String>,
}
impl TargetQuery {
    pub fn named(label: &str) -> Self {
        Self {
            label: label.into(),
            role: None,
            container: None,
            learned_alias: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSnapshot {
    pub window: WindowIdentity,
    pub runtime_id: Vec<i32>,
    pub automation_id: String,
    pub generation: u64,
}
#[derive(Debug, Clone)]
pub struct Candidate {
    pub target: TargetSnapshot,
    pub label: String,
    pub role: String,
    pub container: String,
    pub enabled: bool,
    pub visible: bool,
    pub password: bool,
    pub permitted: bool,
}
impl Candidate {
    pub fn eligible(&self) -> bool {
        self.enabled
            && self.visible
            && !self.password
            && self.permitted
            && !self.target.runtime_id.is_empty()
    }
}
#[derive(Debug, Clone)]
pub enum Resolution {
    Unique(Candidate),
    Clarify(Vec<Candidate>),
    Unavailable,
    Blocked(&'static str),
}

pub fn normalize(label: &str) -> String {
    // UIA names can contain Windows mnemonic markers; && is a literal ampersand.
    let mut out = String::new();
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
        } else {
            out.push(c)
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn allowed_query(q: &TargetQuery, c: &Candidate) -> bool {
    c.eligible()
        && q.role
            .as_ref()
            .is_none_or(|r| normalize(r) == normalize(&c.role))
        && q.container
            .as_ref()
            .is_none_or(|r| normalize(r) == normalize(&c.container))
}
fn select(c: Vec<Candidate>) -> Resolution {
    match c.len() {
        0 => Resolution::Unavailable,
        1 => Resolution::Unique(c.into_iter().next().unwrap()),
        2..=5 => Resolution::Clarify(c),
        _ => Resolution::Blocked("too_many_matches"),
    }
}
/// Aliases are explicit per executable, never fuzzy/model-generated target guesses.
pub fn resolve(
    q: &TargetQuery,
    candidates: &[Candidate],
    learned_id: Option<&str>,
    aliases: &[(&str, &str, &str)],
) -> Resolution {
    if q.label.trim().is_empty() || q.label.len() > 160 {
        return Resolution::Blocked("invalid_target");
    }
    let eligible: Vec<_> = candidates
        .iter()
        .filter(|c| allowed_query(q, c))
        .cloned()
        .collect();
    if let Some(id) = learned_id {
        // A stale learned ID must not fall through to a similarly named control.
        return select(
            eligible
                .into_iter()
                .filter(|c| c.target.automation_id == id)
                .collect(),
        );
    }
    let exact: Vec<_> = eligible
        .iter()
        .filter(|c| c.label.eq_ignore_ascii_case(&q.label))
        .cloned()
        .collect();
    if !exact.is_empty() {
        return select(exact);
    }
    let normalized: Vec<_> = eligible
        .iter()
        .filter(|c| normalize(&c.label) == normalize(&q.label))
        .cloned()
        .collect();
    if !normalized.is_empty() {
        return select(normalized);
    }
    let alias: Vec<_> = eligible
        .into_iter()
        .filter(|c| {
            aliases.iter().any(|(exe, spoken, label)| {
                c.target.window.exe.eq_ignore_ascii_case(exe)
                    && normalize(spoken) == normalize(&q.label)
                    && normalize(label) == normalize(&c.label)
            })
        })
        .collect();
    select(alias)
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectState {
    pub window: WindowIdentity,
    pub toggle: Option<i32>,
    pub expanded: Option<i32>,
    pub selected: Option<bool>,
    pub value_hash: Option<String>,
    pub scroll: Option<f64>,
    pub tab_ids: Option<Vec<Vec<i32>>>,
    pub selected_tab: Option<Vec<i32>>,
    pub scroll_target: Option<Vec<i32>>,
    pub document_hash: Option<String>,
    pub dialog_ids: Option<Vec<Vec<i32>>>,
    pub surfaces: Vec<String>,
}
impl EffectState {
    pub fn empty(window: WindowIdentity) -> Self {
        Self {
            window,
            toggle: None,
            expanded: None,
            selected: None,
            value_hash: None,
            scroll: None,
            scroll_target: None,
            tab_ids: None,
            selected_tab: None,
            document_hash: None,
            dialog_ids: None,
            surfaces: vec![],
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectSpec {
    Focus(WindowIdentity),
    Toggle(i32),
    Expand(i32),
    Select,
    Text(String),
    Scroll(bool),
    TabCount(i32),
    TabSelected(Option<usize>),
    DocumentChanged,
    DialogOpened(String),
    Unsupported,
}
impl EffectSpec {
    pub fn budget_ms(&self) -> u64 {
        if matches!(self, Self::Focus(_)) {
            FOCUS_MS
        } else {
            VERIFY_MS
        }
    }
    pub fn observed(&self, before: &EffectState, after: &EffectState) -> bool {
        if let Self::Focus(expected) = self {
            return &after.window == expected;
        }
        if before.window != after.window {
            return false;
        }
        match self {
            Self::Toggle(state) => after.toggle == Some(*state),
            Self::Expand(state) => after.expanded == Some(*state),
            Self::Select => after.selected == Some(true),
            Self::Text(hash) => after.value_hash.as_ref() == Some(hash),
            Self::Scroll(down) => match (before.scroll, after.scroll) {
                (Some(a), Some(b))
                    if a.is_finite()
                        && b.is_finite()
                        && before.scroll_target.is_some()
                        && before.scroll_target == after.scroll_target =>
                {
                    if *down {
                        b > a
                    } else {
                        b < a
                    }
                }
                _ => false,
            },
            Self::TabCount(delta) => match (&before.tab_ids, &after.tab_ids) {
                (Some(a), Some(b)) => b.len() as i64 - a.len() as i64 == *delta as i64,
                _ => false,
            },
            Self::TabSelected(index) => match (&after.tab_ids, &after.selected_tab) {
                (Some(tabs), Some(id)) => {
                    if let Some(i) = index {
                        tabs.get(*i) == Some(id)
                    } else {
                        before.selected_tab.as_ref().is_some_and(|old| old != id)
                    }
                }
                _ => false,
            },
            Self::DocumentChanged => match (&before.document_hash, &after.document_hash) {
                (Some(a), Some(b)) => a != b,
                _ => false,
            },
            Self::DialogOpened(name) => {
                after.surfaces.contains(&normalize(name))
                    && !before.surfaces.contains(&normalize(name))
            }
            _ => false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Verified,
    Unconfirmed,
    Stopped,
    ForegroundChanged,
}
pub trait Observer {
    fn now_ms(&self) -> u64;
    fn sleep_ms(&self, ms: u64);
    fn cancelled(&self) -> bool;
    fn current_window(&self) -> Option<WindowIdentity>;
    fn observe(&self) -> Option<EffectState>;
}
pub fn verify(env: &dyn Observer, before: &EffectState, effect: &EffectSpec) -> Verification {
    if matches!(effect, EffectSpec::Unsupported) {
        return Verification::Unconfirmed;
    }
    let start = env.now_ms();
    loop {
        if env.cancelled() {
            return Verification::Stopped;
        }
        if env.current_window().as_ref() != Some(&before.window) {
            return Verification::ForegroundChanged;
        }
        if let Some(after) = env.observe() {
            if env.cancelled() {
                return Verification::Stopped;
            }
            if env.current_window().as_ref() != Some(&before.window) {
                return Verification::ForegroundChanged;
            }
            if env.now_ms().saturating_sub(start) > effect.budget_ms() {
                return Verification::Unconfirmed;
            }
            if effect.observed(before, &after) {
                return Verification::Verified;
            }
        }
        let age = env.now_ms().saturating_sub(start);
        if age >= effect.budget_ms() {
            return Verification::Unconfirmed;
        }
        env.sleep_ms(POLL_MS.min(effect.budget_ms() - age));
    }
}

/// Verify an exact insertion/replacement without retaining text or guessing cursor coordinates.
pub fn insertion_matches(
    before: &str,
    after: &str,
    inserted: &str,
    selection: Option<&str>,
) -> bool {
    if before.len() > 1_000_000 || after.len() > 1_000_000 || inserted.is_empty() {
        return false;
    }
    let selected = selection.unwrap_or("");
    before
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(before.len()))
        .any(|i| {
            before[i..].starts_with(selected)
                && after.len() == before.len() - selected.len() + inserted.len()
                && after.starts_with(&before[..i])
                && after[i..].starts_with(inserted)
                && after[i + inserted.len()..] == before[i + selected.len()..]
        })
}

#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    pub number: usize,
    pub label: String,
    pub role: String,
    pub container: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ChoiceView {
    pub token: String,
    pub expires_in_ms: u64,
    pub choices: Vec<Choice>,
}
pub struct PendingChoice {
    pub token: String,
    pub profile: String,
    pub created: u64,
    pub candidates: Vec<Candidate>,
}
impl PendingChoice {
    pub fn view(&self, now: u64) -> Option<ChoiceView> {
        let first = self.candidates.first()?;
        if !(2..=5).contains(&self.candidates.len())
            || first.target.generation == 0
            || self.candidates.iter().any(|c| {
                c.target.window != first.target.window
                    || c.target.generation != first.target.generation
            })
        {
            return None;
        }
        let age = now.saturating_sub(self.created);
        if age >= CHOICE_MS {
            return None;
        }
        let safe = |v: &str| v.chars().filter(|c| !c.is_control()).take(80).collect();
        Some(ChoiceView {
            token: self.token.clone(),
            expires_in_ms: CHOICE_MS - age,
            choices: self
                .candidates
                .iter()
                .enumerate()
                .map(|(i, c)| Choice {
                    number: i + 1,
                    label: safe(&c.label),
                    role: safe(&c.role),
                    container: safe(&c.container),
                })
                .collect(),
        })
    }
}
#[derive(Default)]
pub struct ChoiceSlot {
    pending: Option<PendingChoice>,
}
impl ChoiceSlot {
    pub fn replace(&mut self, p: PendingChoice) {
        self.pending = Some(p)
    }
    pub fn clear(&mut self) {
        self.pending = None
    }
    pub fn view(&self, now: u64) -> Option<ChoiceView> {
        self.pending.as_ref()?.view(now)
    }
    pub fn window(&self) -> Option<&WindowIdentity> {
        Some(&self.pending.as_ref()?.candidates.first()?.target.window)
    }
    pub fn take(
        &mut self,
        token: &str,
        index: usize,
        profile: &str,
        now: u64,
        current: &WindowIdentity,
    ) -> Result<Candidate, &'static str> {
        let p = self.pending.as_ref().ok_or("no_pending_choice")?;
        if p.token != token {
            return Err("stale_choice");
        }
        // Consume before dispatch/revalidation: callbacks cannot replay a choice.
        let p = self.pending.take().unwrap();
        if p.profile != profile || p.view(now).is_none() {
            return Err("expired_choice");
        }
        let c = p
            .candidates
            .get(index.checked_sub(1).ok_or("invalid_choice")?)
            .ok_or("invalid_choice")?;
        if &c.target.window != current || !c.eligible() {
            return Err("target_changed");
        }
        Ok(c.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn win() -> WindowIdentity {
        WindowIdentity {
            hwnd: 1,
            pid: 2,
            created: 3,
            exe: "msedge.exe".into(),
        }
    }
    fn c(label: &str, id: i32) -> Candidate {
        Candidate {
            target: TargetSnapshot {
                window: win(),
                runtime_id: vec![id],
                automation_id: id.to_string(),
                generation: 1,
            },
            label: label.into(),
            role: "button".into(),
            container: "Toolbar".into(),
            enabled: true,
            visible: true,
            password: false,
            permitted: true,
        }
    }
    #[test]
    fn mnemonic_and_whitespace_resolution() {
        assert!(matches!(
            resolve(
                &TargetQuery::named("settings"),
                &[c(" &Settings ", 1)],
                None,
                &[]
            ),
            Resolution::Unique(_)
        ));
        assert_eq!(normalize("A && B"), "a & b");
    }
    #[test]
    fn substring_is_not_a_match() {
        assert!(matches!(
            resolve(&TargetQuery::named("set"), &[c("Settings", 1)], None, &[]),
            Resolution::Unavailable
        ));
    }
    #[test]
    fn duplicate_labels_need_choices() {
        assert!(
            matches!(resolve(&TargetQuery::named("Open"),&[c("Open",1),c("Open",2)],None,&[]),Resolution::Clarify(v) if v.len()==2)
        );
    }
    #[test]
    fn excessive_candidates_require_refinement() {
        assert!(matches!(
            resolve(
                &TargetQuery::named("Open"),
                &(1..=6).map(|i| c("Open", i)).collect::<Vec<_>>(),
                None,
                &[]
            ),
            Resolution::Blocked(_)
        ));
    }
    #[test]
    fn protected_or_disabled_never_match() {
        for flag in 0..4 {
            let mut a = c("Open", 1);
            match flag {
                0 => a.password = true,
                1 => a.enabled = false,
                2 => a.visible = false,
                _ => a.permitted = false,
            };
            assert!(matches!(
                resolve(&TargetQuery::named("Open"), &[a], None, &[]),
                Resolution::Unavailable
            ));
        }
    }
    #[test]
    fn qualifiers_and_aliases_are_exact() {
        let mut a = c("Preferences", 1);
        a.container = "Page".into();
        let mut q = TargetQuery::named("settings");
        assert!(matches!(
            resolve(
                &q,
                &[a.clone()],
                None,
                &[("msedge.exe", "settings", "preferences")]
            ),
            Resolution::Unique(_)
        ));
        q.container = Some("Toolbar".into());
        assert!(matches!(
            resolve(&q, &[a], None, &[]),
            Resolution::Unavailable
        ));
    }
    #[test]
    fn learned_id_does_not_fall_back() {
        assert!(matches!(
            resolve(
                &TargetQuery::named("Open"),
                &[c("Open", 1)],
                Some("missing"),
                &[]
            ),
            Resolution::Unavailable
        ));
    }
    #[test]
    fn unrelated_window_cannot_prove_effect() {
        let a = EffectState::empty(win());
        let mut b = a.clone();
        b.window.hwnd = 9;
        b.selected = Some(true);
        assert!(!EffectSpec::Select.observed(&a, &b));
    }
    #[test]
    fn effect_probes_require_expected_state() {
        let a = EffectState::empty(win());
        let mut b = a.clone();
        b.toggle = Some(1);
        b.expanded = Some(1);
        b.selected = Some(true);
        b.value_hash = Some("hash".into());
        b.scroll = Some(2.0);
        assert!(EffectSpec::Toggle(1).observed(&a, &b));
        assert!(EffectSpec::Expand(1).observed(&a, &b));
        assert!(!EffectSpec::Expand(0).observed(&a, &b));
        assert!(EffectSpec::Select.observed(&a, &b));
        assert!(EffectSpec::Text("hash".into()).observed(&a, &b));
        assert!(!EffectSpec::Scroll(true).observed(&a, &b));
        let mut a = a;
        a.scroll = Some(1.0);
        a.scroll_target = Some(vec![1]);
        b.scroll_target = Some(vec![1]);
        assert!(EffectSpec::Scroll(true).observed(&a, &b));
    }
    #[test]
    fn stale_or_replayed_choice_cannot_dispatch() {
        let mut slot = ChoiceSlot::default();
        slot.replace(PendingChoice {
            token: "t".into(),
            profile: "p".into(),
            created: 1,
            candidates: vec![c("Open", 1), c("Open", 2)],
        });
        assert!(slot.take("old", 1, "p", 2, &win()).is_err());
        assert!(slot.take("t", 1, "p", 2, &win()).is_ok());
        assert!(slot.take("t", 1, "p", 2, &win()).is_err());
    }
    #[test]
    fn mixed_observation_generations_are_not_presented() {
        let mut b = c("Open", 2);
        b.target.generation = 2;
        let p = PendingChoice {
            token: "t".into(),
            profile: "p".into(),
            created: 1,
            candidates: vec![c("Open", 1), b],
        };
        assert!(p.view(2).is_none());
    }
    #[test]
    fn expiry_profile_and_window_changes_reject() {
        for n in 0..3 {
            let mut slot = ChoiceSlot::default();
            slot.replace(PendingChoice {
                token: "t".into(),
                profile: "p".into(),
                created: 1,
                candidates: vec![c("Open", 1), c("Open", 2)],
            });
            let mut w = win();
            if n == 2 {
                w.created = 4
            }
            assert!(slot
                .take(
                    "t",
                    1,
                    if n == 1 { "other" } else { "p" },
                    if n == 0 { 30_001 } else { 2 },
                    &w
                )
                .is_err());
        }
    }
    #[test]
    fn text_insertion_is_exact_unicode_and_selection_aware() {
        assert!(insertion_matches("ab", "a界b", "界", None));
        assert!(insertion_matches("aoldb", "anewb", "new", Some("old")));
        assert!(!insertion_matches("ab", "garbage界", "界", None));
        assert!(!insertion_matches("ab", "ab", "", None));
    }
    struct TestObserver {
        time: std::cell::Cell<u64>,
        delay: u64,
        cancel_at: u64,
        change_at: u64,
        late: bool,
    }
    impl Observer for TestObserver {
        fn now_ms(&self) -> u64 {
            self.time.get()
        }
        fn sleep_ms(&self, ms: u64) {
            self.time.set(self.time.get() + ms)
        }
        fn cancelled(&self) -> bool {
            self.time.get() >= self.cancel_at
        }
        fn current_window(&self) -> Option<WindowIdentity> {
            let mut w = win();
            if self.time.get() >= self.change_at {
                w.hwnd = 99
            }
            Some(w)
        }
        fn observe(&self) -> Option<EffectState> {
            let mut s = EffectState::empty(win());
            if self.time.get() >= self.delay {
                s.toggle = Some(1)
            }
            if self.late {
                self.time.set(self.time.get() + 3000)
            }
            Some(s)
        }
    }
    fn obs(delay: u64) -> TestObserver {
        TestObserver {
            time: std::cell::Cell::new(0),
            delay,
            cancel_at: u64::MAX,
            change_at: u64::MAX,
            late: false,
        }
    }
    #[test]
    fn delayed_effect_is_polled_until_observed() {
        let o = obs(750);
        assert_eq!(
            verify(&o, &EffectState::empty(win()), &EffectSpec::Toggle(1)),
            Verification::Verified
        );
        assert_eq!(o.time.get(), 750)
    }
    #[test]
    fn timeout_does_not_become_success_or_retry() {
        let o = obs(3000);
        assert_eq!(
            verify(&o, &EffectState::empty(win()), &EffectSpec::Toggle(1)),
            Verification::Unconfirmed
        );
        assert_eq!(o.time.get(), 2000)
    }
    #[test]
    fn cancellation_interrupts_verification() {
        let mut o = obs(750);
        o.cancel_at = 100;
        assert_eq!(
            verify(&o, &EffectState::empty(win()), &EffectSpec::Toggle(1)),
            Verification::Stopped
        )
    }
    #[test]
    fn focus_loss_interrupts_verification() {
        let mut o = obs(750);
        o.change_at = 100;
        assert_eq!(
            verify(&o, &EffectState::empty(win()), &EffectSpec::Toggle(1)),
            Verification::ForegroundChanged
        )
    }
    #[test]
    fn late_provider_observation_is_discarded() {
        let mut o = obs(0);
        o.late = true;
        assert_eq!(
            verify(&o, &EffectState::empty(win()), &EffectSpec::Toggle(1)),
            Verification::Unconfirmed
        )
    }
    #[test]
    fn already_satisfied_state_is_verified_without_dispatch() {
        let o = obs(0);
        assert_eq!(
            verify(&o, &EffectState::empty(win()), &EffectSpec::Toggle(1)),
            Verification::Verified
        );
        assert_eq!(o.time.get(), 0)
    }
    #[test]
    fn unsupported_probe_is_unconfirmed_without_waiting() {
        let o = obs(0);
        assert_eq!(
            verify(&o, &EffectState::empty(win()), &EffectSpec::Unsupported),
            Verification::Unconfirmed
        );
        assert_eq!(o.time.get(), 0)
    }
    #[test]
    fn browser_effect_is_specific_to_requested_tab() {
        let mut a = EffectState::empty(win());
        a.tab_ids = Some(vec![vec![1], vec![2]]);
        a.selected_tab = Some(vec![1]);
        let mut b = a.clone();
        b.selected_tab = Some(vec![2]);
        assert!(EffectSpec::TabSelected(Some(1)).observed(&a, &b));
        assert!(!EffectSpec::TabSelected(Some(0)).observed(&a, &b));
        assert!(!EffectSpec::TabCount(1).observed(&a, &b));
        b.tab_ids.as_mut().unwrap().push(vec![3]);
        assert!(EffectSpec::TabCount(1).observed(&a, &b));
    }
    #[test]
    fn dialog_probe_checks_the_expected_name() {
        let a = EffectState::empty(win());
        let mut b = a.clone();
        b.surfaces.push("other".into());
        assert!(!EffectSpec::DialogOpened("Open".into()).observed(&a, &b));
        b.surfaces.push("open".into());
        assert!(EffectSpec::DialogOpened("Open".into()).observed(&a, &b));
    }
}
