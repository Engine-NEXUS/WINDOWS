//! Tauri commands behind the Hub's "Actions & Permissions" page (docs/features/140, P5).
//! Only the local Hub window calls these; voice can never edit the policy.

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use super::audit::{self, AuditEntry};
use super::files::{self, Ctx};
use super::journal::{self, QEntry};
use super::policy::{self, Policy, PolicyEnv};
use super::Origin;

#[derive(Serialize)]
pub struct PolicyView {
    pub policy: Policy,
    /// The folders the voice layer may work in right now (canonical, existing).
    pub roots: Vec<String>,
}

fn data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn actions_get_policy<R: Runtime>(app: AppHandle<R>) -> Result<PolicyView, String> {
    let dir = data_dir(&app)?;
    let policy = policy::load(&dir);
    let roots = policy::effective_roots(&policy).iter().map(|r| r.display().to_string()).collect();
    Ok(PolicyView { policy, roots })
}

#[tauri::command]
pub fn actions_set_policy<R: Runtime>(app: AppHandle<R>, policy: Policy) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let env = PolicyEnv::detect(&dir);
    let clean = policy::sanitize(policy, &env)?;
    policy::save(&dir, &clean)?;
    super::voice::set_extra_aliases(&clean.extra_roots);
    audit::record(
        &dir,
        &AuditEntry::new(chrono::Utc::now().timestamp(), "", "policy_edit", "reversible", "hub", "allow", "verified", &format!("mode={:?} roots={}", clean.mode, clean.extra_roots.len())),
    );
    Ok(())
}

#[tauri::command]
pub fn actions_audit<R: Runtime>(app: AppHandle<R>, limit: Option<usize>) -> Result<Vec<AuditEntry>, String> {
    Ok(audit::tail(&data_dir(&app)?, limit.unwrap_or(40).min(500)))
}

#[tauri::command]
pub fn actions_trash_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<QEntry>, String> {
    let j = journal::load(&data_dir(&app)?);
    Ok(j.waiting().into_iter().cloned().collect())
}

fn with_ctx<R: Runtime, T>(app: &AppHandle<R>, phrase: &str, f: impl FnOnce(&Ctx) -> T) -> Result<T, String> {
    let dir = data_dir(app)?;
    let policy = policy::load(&dir);
    let env = PolicyEnv::detect(&dir);
    let roots = policy::effective_roots(&policy);
    let ctx = Ctx { dir: &dir, policy: &policy, env: &env, roots: &roots, now: chrono::Utc::now().timestamp(), phrase, origin: Origin::UserVoice };
    Ok(f(&ctx))
}

#[tauri::command]
pub fn actions_restore<R: Runtime>(app: AppHandle<R>, id: String) -> Result<String, String> {
    with_ctx(&app, "hub: restore", |ctx| files::restore_entry(ctx, &id).spoken)
}

#[tauri::command]
pub fn actions_undo<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    with_ctx(&app, "hub: undo", |ctx| files::undo_last(ctx).spoken)
}
