//! Work that keeps running after the turn that started it is over (a CodeRabbit review takes 2 to 15 minutes, an
//! AI analysis a minute or two).
//!
//! The turn spinner cannot show these: it belongs to a request, and the request is cleared the moment the spoken
//! "On it, sir" ends the turn (`show_loading` refuses an orphan spinner, and the frontend's loading failsafe gives up
//! after 120 s). So nothing showed that a review was still running, and nothing cleared it (2026-10-29 loading-state
//! research). This registry is the one list of such tasks; the stage draws a small chip for each entry and the job
//! itself removes its entry when it finishes, fails or is cancelled.

use serde::Serialize;
use std::sync::Mutex;

/// A task nobody finishes (a panicked job) must not show a spinner forever. Reviews take up to 15 minutes.
pub const MAX_AGE_MS: u64 = 20 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    /// "coderabbit" or "ai-review": picks the wording, never the behaviour.
    pub kind: String,
    pub label: String,
    pub started_ms: u64,
}

static TASKS: Mutex<Vec<Task>> = Mutex::new(Vec::new());

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Insert or replace by id. Pure.
pub fn upsert(list: &mut Vec<Task>, task: Task) {
    match list.iter_mut().find(|t| t.id == task.id) {
        Some(existing) => *existing = task,
        None => list.push(task),
    }
}

/// Remove by id; true when something was removed. Pure.
pub fn remove(list: &mut Vec<Task>, id: &str) -> bool {
    let before = list.len();
    list.retain(|t| t.id != id);
    list.len() != before
}

/// Drop entries older than `MAX_AGE_MS`. Pure.
pub fn prune(list: &mut Vec<Task>, now: u64) {
    list.retain(|t| now.saturating_sub(t.started_ms) <= MAX_AGE_MS);
}

fn snapshot() -> Vec<Task> {
    let mut g = TASKS.lock().unwrap_or_else(|e| e.into_inner());
    prune(&mut g, now_ms());
    g.clone()
}

fn publish<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    crate::commands::emit_logged(app, "task:update", snapshot());
}

/// A background task has started: show its chip.
pub fn start<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str, kind: &str, label: &str) {
    {
        let mut g = TASKS.lock().unwrap_or_else(|e| e.into_inner());
        upsert(&mut g, Task { id: id.to_string(), kind: kind.to_string(), label: label.to_string(), started_ms: now_ms() });
    }
    tracing::info!("background task started: {id}");
    publish(app);
}

/// The task is over (done, failed or cancelled): remove its chip. Safe to call twice.
pub fn finish<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str) {
    let removed = remove(&mut TASKS.lock().unwrap_or_else(|e| e.into_inner()), id);
    if removed {
        tracing::info!("background task finished: {id}");
    }
    publish(app);
}

/// IPC: what is running now (the stage asks once on mount; later changes arrive as `task:update`).
#[tauri::command]
pub fn background_tasks_list() -> Vec<Task> {
    snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, started_ms: u64) -> Task {
        Task { id: id.into(), kind: "coderabbit".into(), label: "x".into(), started_ms }
    }

    #[test]
    fn upsert_adds_then_replaces_by_id() {
        let mut l = vec![];
        upsert(&mut l, t("a", 1));
        upsert(&mut l, t("b", 2));
        upsert(&mut l, t("a", 9));
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].started_ms, 9, "a repeat start of the same task restarts its clock, it does not add a chip");
    }

    #[test]
    fn remove_reports_whether_anything_went() {
        let mut l = vec![t("a", 1)];
        assert!(remove(&mut l, "a"));
        assert!(!remove(&mut l, "a"), "finishing twice is harmless");
        assert!(l.is_empty());
    }

    #[test]
    fn a_task_nobody_finished_expires() {
        let mut l = vec![t("old", 0), t("new", MAX_AGE_MS)];
        prune(&mut l, MAX_AGE_MS + 1);
        assert_eq!(l.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["new"]);
        let mut edge = vec![t("edge", 0)];
        prune(&mut edge, MAX_AGE_MS);
        assert_eq!(edge.len(), 1, "exactly at the limit still shows");
    }
}
