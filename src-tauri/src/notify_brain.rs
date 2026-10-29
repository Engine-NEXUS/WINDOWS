//! Notification state machine — failed / still failing / fixed (task P1).
//!
//! A connected service must never repeat itself and must report recovery.
//! Per entity (e.g. "github:zync:build") the brain tracks state (`ok` or `failing`).
//! The first failure is spoken once, further failures stay silent, recovery is spoken
//! once ("zync is green again"), and a failure that continues is mentioned again only
//! after a long lull (`STILL_FAILING_MS = 6 h`).

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime};

/// Time after which an unconfirmed failure is forgotten and treated as ok (12 hours).
pub const FAIL_FORGET_MS: u64 = 12 * 3600 * 1000;

/// Interval before a continuing failure is spoken again (6 hours).
pub const STILL_FAILING_MS: u64 = 6 * 3600 * 1000;

/// Retention period for entities in the in-memory book and store (30 days).
pub const RETENTION_MS: u64 = 30 * 24 * 3600 * 1000;

/// Maximum entities kept in memory.
const BOOK_CAP: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obs {
    Failed,
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pr,
    Build,
    Deploy,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Pr => "pr",
            Kind::Build => "build",
            Kind::Deploy => "deploy",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub service: &'static str,
    pub project: String,
    pub kind: Kind,
    pub obs: Obs,
    pub run_ref: String,
}

/// Derives the stable entity identifier: `{service}:{project_lowercase}:{pr|build}`.
pub fn entity_id(f: &Fact) -> String {
    format!("{}:{}:{}", f.service, f.project.to_lowercase(), f.kind.as_str())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub id: String,
    pub failing: bool,
    pub since_ms: u64,
    pub fail_count: u32,
    pub last_ref: String,
    pub spoken_failure: bool,
    pub last_spoken_ms: u64,
    pub last_seen_ms: u64,
}

impl From<crate::memcore::store::EntityRecord> for Entity {
    fn from(r: crate::memcore::store::EntityRecord) -> Self {
        Entity {
            id: r.id,
            failing: r.failing,
            since_ms: r.since_ms,
            fail_count: r.fail_count,
            last_ref: r.last_ref,
            spoken_failure: r.spoken_failure,
            last_spoken_ms: r.last_spoken_ms,
            last_seen_ms: r.last_seen_ms,
        }
    }
}

impl From<&Entity> for crate::memcore::store::EntityRecord {
    fn from(e: &Entity) -> Self {
        crate::memcore::store::EntityRecord {
            id: e.id.clone(),
            failing: e.failing,
            since_ms: e.since_ms,
            fail_count: e.fail_count,
            last_ref: e.last_ref.clone(),
            spoken_failure: e.spoken_failure,
            last_spoken_ms: e.last_spoken_ms,
            last_seen_ms: e.last_seen_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Failed { project: String, kind: Kind, count: u32 },
    StillFailing { project: String, kind: Kind, count: u32 },
    Fixed { project: String, kind: Kind },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    Silent,
    Say(Line),
}

pub struct Book {
    entities: HashMap<String, Entity>,
}

impl Book {
    pub fn from_records(records: Vec<Entity>) -> Self {
        let mut entities = HashMap::with_capacity(records.len().min(BOOK_CAP));
        for r in records {
            entities.insert(r.id.clone(), r);
        }
        while entities.len() > BOOK_CAP {
            if let Some(oldest_key) = entities
                .iter()
                .min_by_key(|(_, e)| e.last_seen_ms)
                .map(|(k, _)| k.clone())
            {
                entities.remove(&oldest_key);
            } else {
                break;
            }
        }
        Self { entities }
    }

    #[cfg(test)]
    pub fn records(&self) -> Vec<Entity> {
        self.entities.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Moves an entity to a new (full-identity) key keeping its whole state, so a key migration neither replays
    /// an old failure as new nor forgets one. Returns false (and changes nothing) when `old` is unknown or
    /// `new` is already taken: an existing record is never overwritten.
    #[cfg(test)] // N2 uses it for the full-key migration; test-only until then (dead-code ratchet)
    pub fn rekey(&mut self, old: &str, new: &str) -> bool {
        if old == new || self.entities.contains_key(new) {
            return false;
        }
        match self.entities.remove(old) {
            Some(mut e) => {
                e.id = new.to_string();
                self.entities.insert(new.to_string(), e);
                true
            }
            None => false,
        }
    }

    pub fn prune(&mut self, now_ms: u64) {
        let cutoff = now_ms.saturating_sub(RETENTION_MS);
        self.entities.retain(|_, e| e.last_seen_ms >= cutoff);
    }

    fn insert_entity(&mut self, entry: Entity) {
        if !self.entities.contains_key(&entry.id) && self.entities.len() >= BOOK_CAP {
            if let Some(oldest_key) = self
                .entities
                .iter()
                .min_by_key(|(_, e)| e.last_seen_ms)
                .map(|(k, _)| k.clone())
            {
                self.entities.remove(&oldest_key);
            }
        }
        self.entities.insert(entry.id.clone(), entry);
    }

    pub fn observe(&mut self, f: &Fact, now_ms: u64, baseline: bool) -> Decision {
        let id = entity_id(f);
        let is_unknown = !self.entities.contains_key(&id);
        let is_forgotten = if let Some(existing) = self.entities.get(&id) {
            existing.failing && now_ms.saturating_sub(existing.last_seen_ms) >= FAIL_FORGET_MS
        } else {
            false
        };

        let treating_as_ok = is_unknown || is_forgotten || {
            let existing = self.entities.get(&id).unwrap();
            !existing.failing
        };

        if treating_as_ok {
            match f.obs {
                Obs::Failed => {
                    // Row 2: ok + Failed
                    let (decision, spoken, last_spoken) = if baseline {
                        (Decision::Silent, false, 0u64)
                    } else {
                        (
                            Decision::Say(Line::Failed {
                                project: f.project.clone(),
                                kind: f.kind,
                                count: 1,
                            }),
                            true,
                            now_ms,
                        )
                    };
                    let entry = Entity {
                        id: id.clone(),
                        failing: true,
                        since_ms: now_ms,
                        fail_count: 1,
                        last_ref: f.run_ref.clone(),
                        spoken_failure: spoken,
                        last_spoken_ms: last_spoken,
                        last_seen_ms: now_ms,
                    };
                    self.insert_entity(entry);
                    decision
                }
                Obs::Fixed => {
                    // Row 6: ok + Fixed
                    let entry = Entity {
                        id: id.clone(),
                        failing: false,
                        since_ms: 0,
                        fail_count: 0,
                        last_ref: f.run_ref.clone(),
                        spoken_failure: false,
                        last_spoken_ms: 0,
                        last_seen_ms: now_ms,
                    };
                    self.insert_entity(entry);
                    Decision::Silent
                }
            }
        } else {
            let entity = self.entities.get_mut(&id).unwrap();
            entity.last_seen_ms = now_ms;

            match f.obs {
                Obs::Failed => {
                    if f.run_ref == entity.last_ref {
                        // Row 3: failing + Failed with same run_ref -> Silent (only refresh last_seen_ms)
                        Decision::Silent
                    } else {
                        // Row 4: failing + Failed with new run_ref
                        entity.fail_count += 1;
                        entity.last_ref = f.run_ref.clone();

                        if entity.spoken_failure
                            && now_ms.saturating_sub(entity.last_spoken_ms) >= STILL_FAILING_MS
                        {
                            entity.last_spoken_ms = now_ms;
                            entity.spoken_failure = true;
                            Decision::Say(Line::StillFailing {
                                project: f.project.clone(),
                                kind: f.kind,
                                count: entity.fail_count,
                            })
                        } else if !entity.spoken_failure && !baseline {
                            entity.last_spoken_ms = now_ms;
                            entity.spoken_failure = true;
                            Decision::Say(Line::Failed {
                                project: f.project.clone(),
                                kind: f.kind,
                                count: entity.fail_count,
                            })
                        } else {
                            Decision::Silent
                        }
                    }
                }
                Obs::Fixed => {
                    // Row 5: failing + Fixed
                    let say_fixed = entity.spoken_failure && !baseline;
                    entity.failing = false;
                    entity.fail_count = 0;
                    entity.last_ref = f.run_ref.clone();
                    entity.spoken_failure = false;

                    if say_fixed {
                        Decision::Say(Line::Fixed {
                            project: f.project.clone(),
                            kind: f.kind,
                        })
                    } else {
                        Decision::Silent
                    }
                }
            }
        }
    }
}

fn capitalize_service(service: &str) -> String {
    let mut chars = service.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn render_for(service: &str, line: &Line, address: Option<&str>) -> String {
    let sentence = match line {
        Line::Failed { project, kind, .. } => match kind {
            Kind::Pr => format!("PR failed in {project}"),
            Kind::Build => format!("Build failed in {project}"),
            Kind::Deploy if service == "supabase" => format!("Supabase project down in {project}"),
            Kind::Deploy => {
                let svc = capitalize_service(service);
                format!("{svc} deploy failed in {project}")
            }
        },
        Line::StillFailing { project, kind, count } => match kind {
            Kind::Pr => {
                if *count > 2 {
                    format!("PR checks still failing in {project}, {count} runs")
                } else {
                    format!("PR checks still failing in {project}")
                }
            }
            Kind::Build => {
                if *count > 2 {
                    format!("Build still failing in {project}, {count} runs")
                } else {
                    format!("Build still failing in {project}")
                }
            }
            Kind::Deploy if service == "supabase" => {
                if *count > 2 {
                    format!("Supabase project still down in {project}, {count} checks")
                } else {
                    format!("Supabase project still down in {project}")
                }
            }
            Kind::Deploy => {
                let svc = capitalize_service(service);
                if *count > 2 {
                    format!("{svc} deploy still failing in {project}, {count} deploys")
                } else {
                    format!("{svc} deploy still failing in {project}")
                }
            }
        },
        Line::Fixed { project, kind } => match kind {
            Kind::Deploy if service == "supabase" => format!("{project} is healthy again"),
            Kind::Deploy => format!("{project} is live again"),
            _ => format!("{project} is green again"),
        },
    };
    crate::memcore::mailwording::tail(address, &sentence)
}

#[cfg(test)]
pub fn render(line: &Line, address: Option<&str>) -> String {
    render_for("github", line, address)
}

// ─── Process Glue ────────────────────────────────────────────────────────────

static BOOK: Mutex<Option<Book>> = Mutex::new(None);

pub fn observe_fact<R: Runtime>(app: &AppHandle<R>, f: &Fact, baseline: bool) -> Decision {
    let Ok(dir) = app.path().app_data_dir() else {
        return Decision::Silent;
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut guard = BOOK.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let records = crate::memcore::with_store(&dir, |s| s.entities_load()).unwrap_or_default();
        let entities: Vec<Entity> = records.into_iter().map(Entity::from).collect();
        let mut b = Book::from_records(entities);
        b.prune(now_ms);
        *guard = Some(b);
    }

    let book = guard.as_mut().unwrap();
    let decision = book.observe(f, now_ms, baseline);

    let eid = entity_id(f);
    if let Some(entity) = book.get(&eid) {
        let rec = crate::memcore::store::EntityRecord::from(entity);
        let _ = crate::memcore::with_store(&dir, |s| s.entity_save(&rec));
    }

    decision
}

pub fn say<R: Runtime>(app: &AppHandle<R>, service: &str, line: Line) {
    let friend = crate::persona::is_friend(&crate::commands::read_persona_mode(app));
    let name = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| crate::memory::read_user_profile(&d))
        .and_then(|p| p.name);
    let address = crate::persona::address(name.as_deref(), friend);
    let text = render_for(service, &line, address.as_deref());
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let (project, kind, tag) = match &line {
        Line::Failed { project, kind, .. } => (project, *kind, "failed"),
        Line::StillFailing { project, kind, .. } => (project, *kind, "still"),
        Line::Fixed { project, kind } => (project, *kind, "fixed"),
    };
    let eid = format!("{}:{}:{}", service.to_lowercase(), project.to_lowercase(), kind.as_str());
    let id = format!("nb_{eid}_{now_ms}");
    let event_key = format!("nb:{eid}:{tag}");

    crate::proactive_policy::submit_event(
        app,
        id,
        text,
        crate::google::types::AlertUrgency::Medium,
        Some(event_key),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failing_entity(id: &str, run: &str) -> Entity {
        Entity {
            id: id.into(),
            failing: true,
            since_ms: 5,
            fail_count: 3,
            last_ref: run.into(),
            spoken_failure: true,
            last_spoken_ms: 9,
            last_seen_ms: 10,
        }
    }

    #[test]
    fn rekey_keeps_the_whole_state_and_leaves_no_legacy_copy() {
        let mut book = Book::from_records(vec![failing_entity("github:zync:pr", "r7")]);
        assert!(book.rekey("github:zync:pr", "v1:github:acct1:github.com/o/zync:pr"));
        assert!(book.get("github:zync:pr").is_none());
        let moved = book.get("v1:github:acct1:github.com/o/zync:pr").unwrap();
        assert_eq!(moved.id, "v1:github:acct1:github.com/o/zync:pr");
        assert_eq!((moved.failing, moved.fail_count, moved.last_ref.as_str(), moved.spoken_failure, moved.last_seen_ms), (true, 3, "r7", true, 10));
    }

    #[test]
    fn rekey_never_overwrites_an_existing_record_or_invents_one() {
        let mut book = Book::from_records(vec![failing_entity("a", "r1"), failing_entity("b", "r2")]);
        assert!(!book.rekey("a", "b"), "destination already taken");
        assert_eq!(book.get("b").unwrap().last_ref, "r2");
        assert!(book.get("a").is_some(), "source untouched on refusal");
        assert!(!book.rekey("missing", "c"));
        assert!(book.get("c").is_none());
        assert!(!book.rekey("a", "a"));
    }

    #[test]
    fn a_rekeyed_failure_does_not_replay_as_a_new_alert() {
        // Same run observed again after the migration (as the next poll would) stays silent.
        let mut book = Book::from_records(vec![failing_entity("github:zync:pr", "r7")]);
        book.rekey("github:zync:pr", "github:zync:pr");
        let f = Fact { service: "github", project: "zync".into(), kind: Kind::Pr, obs: Obs::Failed, run_ref: "r7".into() };
        assert_eq!(book.observe(&f, 20, false), Decision::Silent);
    }

    #[test]
    fn test_entity_id_derivation() {
        let f_pr = Fact {
            service: "github",
            project: "Zync".into(),
            kind: Kind::Pr,
            obs: Obs::Failed,
            run_ref: "101".into(),
        };
        assert_eq!(entity_id(&f_pr), "github:zync:pr");

        let f_build = Fact {
            service: "github",
            project: "Nexus-Engine".into(),
            kind: Kind::Build,
            obs: Obs::Fixed,
            run_ref: "102".into(),
        };
        assert_eq!(entity_id(&f_build), "github:nexus-engine:build");
    }

    #[test]
    fn test_state_table_row1_unknown_treated_as_ok() {
        let mut book = Book::from_records(vec![]);
        let now = 1_000_000u64;
        let f = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r1".into(),
        };
        // Unknown entity + Failed -> Row 2 (treated as ok) -> Say(Failed{count: 1})
        let decision = book.observe(&f, now, false);
        assert_eq!(
            decision,
            Decision::Say(Line::Failed {
                project: "zync".into(),
                kind: Kind::Build,
                count: 1,
            })
        );
    }

    #[test]
    fn test_state_table_row1_forgotten_failing_treated_as_ok() {
        let mut book = Book::from_records(vec![]);
        let now = 1_000_000u64;
        let f1 = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r1".into(),
        };
        book.observe(&f1, now, false);

        // Advance by FAIL_FORGET_MS + 1 second (12 hours + 1s)
        let later = now + FAIL_FORGET_MS + 1000;
        let f2 = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r2".into(),
        };
        // Forgotten failure is treated as ok, so a new failure is news again
        let decision = book.observe(&f2, later, false);
        assert_eq!(
            decision,
            Decision::Say(Line::Failed {
                project: "zync".into(),
                kind: Kind::Build,
                count: 1,
            })
        );
        let e = book.get("github:zync:build").unwrap();
        assert_eq!(e.fail_count, 1);
        assert_eq!(e.since_ms, later);
        assert_eq!(e.last_ref, "r2");
    }

    #[test]
    fn test_state_table_row2_ok_failed_baseline_vs_live() {
        let mut book = Book::from_records(vec![]);
        let now = 1_000_000u64;

        // Baseline: silent and spoken_failure is false
        let f_base = Fact {
            service: "github",
            project: "repo-a".into(),
            kind: Kind::Pr,
            obs: Obs::Failed,
            run_ref: "b1".into(),
        };
        assert_eq!(book.observe(&f_base, now, true), Decision::Silent);
        let e_a = book.get("github:repo-a:pr").unwrap();
        assert!(e_a.failing);
        assert_eq!(e_a.fail_count, 1);
        assert!(!e_a.spoken_failure);

        // Non-baseline: Say(Failed{count: 1}) and spoken_failure is true
        let f_live = Fact {
            service: "github",
            project: "repo-b".into(),
            kind: Kind::Pr,
            obs: Obs::Failed,
            run_ref: "l1".into(),
        };
        assert_eq!(
            book.observe(&f_live, now, false),
            Decision::Say(Line::Failed {
                project: "repo-b".into(),
                kind: Kind::Pr,
                count: 1,
            })
        );
        let e_b = book.get("github:repo-b:pr").unwrap();
        assert!(e_b.failing);
        assert_eq!(e_b.fail_count, 1);
        assert!(e_b.spoken_failure);
        assert_eq!(e_b.last_spoken_ms, now);
    }

    #[test]
    fn test_state_table_row3_failing_failed_same_ref_is_silent() {
        let mut book = Book::from_records(vec![]);
        let now = 1_000_000u64;
        let f = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "same_run".into(),
        };
        book.observe(&f, now, false);

        // Same run_ref seen again 1 minute later
        let d = book.observe(&f, now + 60_000, false);
        assert_eq!(d, Decision::Silent);
        let e = book.get("github:zync:build").unwrap();
        assert_eq!(e.fail_count, 1);
        assert_eq!(e.last_seen_ms, now + 60_000);
    }

    #[test]
    fn test_state_table_row4_failing_failed_new_ref() {
        let mut book = Book::from_records(vec![]);
        let now = 1_000_000u64;

        // Initial failure spoken
        let f1 = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r1".into(),
        };
        book.observe(&f1, now, false);

        // New run_ref after 1 hour (less than 6 hours STILL_FAILING_MS) -> Silent
        let f2 = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r2".into(),
        };
        assert_eq!(book.observe(&f2, now + 3600 * 1000, false), Decision::Silent);
        let e = book.get("github:zync:build").unwrap();
        assert_eq!(e.fail_count, 2);

        // New run_ref after STILL_FAILING_MS (6h) -> Say(StillFailing{count: 3})
        let f3 = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r3".into(),
        };
        let t3 = now + STILL_FAILING_MS;
        assert_eq!(
            book.observe(&f3, t3, false),
            Decision::Say(Line::StillFailing {
                project: "zync".into(),
                kind: Kind::Build,
                count: 3,
            })
        );
        let e = book.get("github:zync:build").unwrap();
        assert_eq!(e.fail_count, 3);
        assert_eq!(e.last_spoken_ms, t3);
    }

    #[test]
    fn test_state_table_row4_failing_failed_unspoken_first_notice() {
        let mut book = Book::from_records(vec![]);
        let now = 1_000_000u64;

        // Baseline failure: spoken_failure is false
        let f1 = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r1".into(),
        };
        assert_eq!(book.observe(&f1, now, true), Decision::Silent);

        // Second failure arrives via live event (!baseline): first time owner hears about it -> Say(Failed{count: 2})
        let f2 = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r2".into(),
        };
        assert_eq!(
            book.observe(&f2, now + 10_000, false),
            Decision::Say(Line::Failed {
                project: "zync".into(),
                kind: Kind::Build,
                count: 2,
            })
        );
        let e = book.get("github:zync:build").unwrap();
        assert!(e.spoken_failure);
        assert_eq!(e.last_spoken_ms, now + 10_000);
    }

    #[test]
    fn test_state_table_row5_failing_fixed() {
        let mut book = Book::from_records(vec![]);
        let now = 1_000_000u64;

        // 1. Spoken failure then Fixed -> Say(Fixed)
        let f_fail = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Pr,
            obs: Obs::Failed,
            run_ref: "r1".into(),
        };
        book.observe(&f_fail, now, false);

        let f_fix = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Pr,
            obs: Obs::Fixed,
            run_ref: "r2".into(),
        };
        assert_eq!(
            book.observe(&f_fix, now + 5000, false),
            Decision::Say(Line::Fixed {
                project: "zync".into(),
                kind: Kind::Pr,
            })
        );
        let e = book.get("github:zync:pr").unwrap();
        assert!(!e.failing);
        assert_eq!(e.fail_count, 0);
        assert!(!e.spoken_failure);

        // 2. Unspoken (baseline) failure then Fixed -> Silent
        let f_base = Fact {
            service: "github",
            project: "app".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "b1".into(),
        };
        book.observe(&f_base, now, true);

        let f_fix_base = Fact {
            service: "github",
            project: "app".into(),
            kind: Kind::Build,
            obs: Obs::Fixed,
            run_ref: "b2".into(),
        };
        assert_eq!(book.observe(&f_fix_base, now + 5000, false), Decision::Silent);
        let e_app = book.get("github:app:build").unwrap();
        assert!(!e_app.failing);

        // 3. Spoken failure then Fixed with baseline=true -> Silent
        let f_fail2 = Fact {
            service: "github",
            project: "repo3".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r3".into(),
        };
        book.observe(&f_fail2, now, false);
        let f_fix_as_base = Fact {
            service: "github",
            project: "repo3".into(),
            kind: Kind::Build,
            obs: Obs::Fixed,
            run_ref: "r4".into(),
        };
        assert_eq!(book.observe(&f_fix_as_base, now + 5000, true), Decision::Silent);
    }

    #[test]
    fn test_state_table_row6_ok_fixed_is_silent() {
        let mut book = Book::from_records(vec![]);
        let now = 1_000_000u64;

        let f_fix = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Fixed,
            run_ref: "r1".into(),
        };
        // Unknown/ok entity receives Fixed -> Silent
        assert_eq!(book.observe(&f_fix, now, false), Decision::Silent);
        let e = book.get("github:zync:build").unwrap();
        assert!(!e.failing);
        assert_eq!(e.fail_count, 0);

        // Second routine success -> still Silent
        assert_eq!(book.observe(&f_fix, now + 1000, false), Decision::Silent);
    }

    #[test]
    fn test_state_table_row7_last_seen_updated_every_call() {
        let mut book = Book::from_records(vec![]);
        let f = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r1".into(),
        };
        book.observe(&f, 100, false);
        assert_eq!(book.get("github:zync:build").unwrap().last_seen_ms, 100);

        book.observe(&f, 200, false);
        assert_eq!(book.get("github:zync:build").unwrap().last_seen_ms, 200);

        let f_fix = Fact {
            service: "github",
            project: "zync".into(),
            kind: Kind::Build,
            obs: Obs::Fixed,
            run_ref: "r2".into(),
        };
        book.observe(&f_fix, 300, false);
        assert_eq!(book.get("github:zync:build").unwrap().last_seen_ms, 300);
    }

    #[test]
    fn test_book_cap_eviction_and_prune() {
        let mut book = Book::from_records(vec![]);

        // Insert 300 entities
        for i in 0..300 {
            let f = Fact {
                service: "github",
                project: format!("proj_{i}"),
                kind: Kind::Build,
                obs: Obs::Failed,
                run_ref: "r".into(),
            };
            book.observe(&f, 1000 + i as u64, false);
        }
        assert_eq!(book.records().len(), 300);

        // 301st entity evicts the oldest last_seen_ms (proj_0 with last_seen_ms = 1000)
        let f301 = Fact {
            service: "github",
            project: "proj_300".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r".into(),
        };
        book.observe(&f301, 2000, false);
        assert_eq!(book.records().len(), 300);
        assert!(book.get("github:proj_0:build").is_none());
        assert!(book.get("github:proj_300:build").is_some());

        // Prune drops entities older than 30 days
        let now_ms = 50 * 24 * 3600 * 1000;
        let f_recent = Fact {
            service: "github",
            project: "proj_fresh".into(),
            kind: Kind::Build,
            obs: Obs::Failed,
            run_ref: "r".into(),
        };
        book.observe(&f_recent, now_ms, false);
        book.prune(now_ms);
        // Only proj_fresh remains because all other 300 entities had last_seen_ms < 2000 ms
        assert_eq!(book.records().len(), 1);
        assert!(book.get("github:proj_fresh:build").is_some());
    }

    #[test]
    fn test_render_all_lines_under_12_words() {
        let lines = vec![
            Line::Failed {
                project: "zync".into(),
                kind: Kind::Pr,
                count: 1,
            },
            Line::Failed {
                project: "zync".into(),
                kind: Kind::Build,
                count: 1,
            },
            Line::StillFailing {
                project: "zync".into(),
                kind: Kind::Pr,
                count: 2,
            },
            Line::StillFailing {
                project: "zync".into(),
                kind: Kind::Pr,
                count: 5,
            },
            Line::StillFailing {
                project: "zync".into(),
                kind: Kind::Build,
                count: 2,
            },
            Line::StillFailing {
                project: "zync".into(),
                kind: Kind::Build,
                count: 10,
            },
            Line::Fixed {
                project: "zync".into(),
                kind: Kind::Pr,
            },
            Line::Fixed {
                project: "zync".into(),
                kind: Kind::Build,
            },
        ];

        let addresses = [None, Some("sir"), Some("Alex")];

        for line in &lines {
            for addr in addresses {
                let rendered = render(line, addr);
                let word_count = rendered.split_whitespace().count();
                assert!(
                    word_count <= 12,
                    "rendered line exceeded 12 words: '{rendered}' ({word_count} words)"
                );
            }
        }

        // Deploy lines across vercel, render, netlify stay <= 12 words
        for svc in ["vercel", "render", "netlify"] {
            let deploy_lines = vec![
                Line::Failed {
                    project: "zync".into(),
                    kind: Kind::Deploy,
                    count: 1,
                },
                Line::StillFailing {
                    project: "zync".into(),
                    kind: Kind::Deploy,
                    count: 2,
                },
                Line::StillFailing {
                    project: "zync".into(),
                    kind: Kind::Deploy,
                    count: 5,
                },
                Line::Fixed {
                    project: "zync".into(),
                    kind: Kind::Deploy,
                },
            ];
            for line in &deploy_lines {
                for addr in addresses {
                    let rendered = render_for(svc, line, addr);
                    let word_count = rendered.split_whitespace().count();
                    assert!(
                        word_count <= 12,
                        "rendered line for {svc} exceeded 12 words: '{rendered}' ({word_count} words)"
                    );
                }
            }
        }

        // Exact rendered string checks
        let failed_pr = Line::Failed {
            project: "zync".into(),
            kind: Kind::Pr,
            count: 1,
        };
        assert_eq!(render(&failed_pr, Some("sir")), "PR failed in zync, sir.");
        assert_eq!(render(&failed_pr, None), "PR failed in zync.");

        let failed_build = Line::Failed {
            project: "zync".into(),
            kind: Kind::Build,
            count: 1,
        };
        assert_eq!(render(&failed_build, Some("sir")), "Build failed in zync, sir.");
        assert_eq!(render(&failed_build, None), "Build failed in zync.");

        let still_pr_2 = Line::StillFailing {
            project: "zync".into(),
            kind: Kind::Pr,
            count: 2,
        };
        assert_eq!(render(&still_pr_2, Some("sir")), "PR checks still failing in zync, sir.");

        let still_pr_5 = Line::StillFailing {
            project: "zync".into(),
            kind: Kind::Pr,
            count: 5,
        };
        assert_eq!(
            render(&still_pr_5, Some("sir")),
            "PR checks still failing in zync, 5 runs, sir."
        );

        let still_build_2 = Line::StillFailing {
            project: "zync".into(),
            kind: Kind::Build,
            count: 2,
        };
        assert_eq!(render(&still_build_2, Some("sir")), "Build still failing in zync, sir.");

        let still_build_3 = Line::StillFailing {
            project: "zync".into(),
            kind: Kind::Build,
            count: 3,
        };
        assert_eq!(
            render(&still_build_3, Some("sir")),
            "Build still failing in zync, 3 runs, sir."
        );

        let fixed = Line::Fixed {
            project: "zync".into(),
            kind: Kind::Build,
        };
        assert_eq!(render(&fixed, Some("sir")), "Zync is green again, sir.");
        assert_eq!(render(&fixed, None), "Zync is green again.");

        // Deploy exact strings
        let failed_deploy = Line::Failed {
            project: "zync".into(),
            kind: Kind::Deploy,
            count: 1,
        };
        assert_eq!(render_for("vercel", &failed_deploy, Some("sir")), "Vercel deploy failed in zync, sir.");
        assert_eq!(render_for("supabase", &failed_deploy, Some("sir")), "Supabase project down in zync, sir.");
        assert_eq!(render_for("vercel", &failed_deploy, None), "Vercel deploy failed in zync.");

        let still_deploy_2 = Line::StillFailing {
            project: "zync".into(),
            kind: Kind::Deploy,
            count: 2,
        };
        assert_eq!(render_for("render", &still_deploy_2, Some("sir")), "Render deploy still failing in zync, sir.");

        let still_deploy_5 = Line::StillFailing {
            project: "zync".into(),
            kind: Kind::Deploy,
            count: 5,
        };
        assert_eq!(
            render_for("netlify", &still_deploy_5, Some("sir")),
            "Netlify deploy still failing in zync, 5 deploys, sir."
        );

        let fixed_deploy = Line::Fixed {
            project: "zync".into(),
            kind: Kind::Deploy,
        };
        assert_eq!(render_for("vercel", &fixed_deploy, Some("sir")), "Zync is live again, sir.");
        assert_eq!(render_for("vercel", &fixed_deploy, None), "Zync is live again.");
    }
}
