//! File tools (docs/features/140, P2). Every tool calls Rust file APIs directly: there is no shell,
//! no command string and no path parsing outside `paths.rs`. Flow: `plan` (canonical paths,
//! measured, policy verdict, read-back preview) -> `execute` (re-check, act, verify, journal,
//! audit) -> `undo_last` / `restore_entry`. Nothing here ever overwrites or permanently deletes.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::audit::{self, AuditEntry};
use super::journal::{self, Batch, Journal, Op, QEntry, TRASH_DIR};
use super::paths::{self, check_name, is_within};
use super::policy::{self, Decision, Policy, PolicyEnv, Target, Verdict};
use super::{Origin, Outcome, Receipt, Risk, Tool};

/// Entries walked when measuring a folder; beyond this the count is reported as "N+".
const MEASURE_CAP: u64 = 10_000;

pub struct Ctx<'a> {
    pub dir: &'a Path,
    pub policy: &'a Policy,
    pub env: &'a PolicyEnv,
    pub roots: &'a [PathBuf],
    pub now: i64,
    pub phrase: &'a str,
    pub origin: Origin,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub tool: Tool,
    pub sources: Vec<PathBuf>,
    /// Folder that receives copies/moves or the new folder.
    pub dest_dir: Option<PathBuf>,
    /// New name for rename / create folder.
    pub new_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct Snap {
    path: PathBuf,
    is_dir: bool,
    bytes: u64,
    files: u64,
    modified: u64,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub tool: Tool,
    pub verdict: Verdict,
    pub phrase: String,
    pub origin: Origin,
    targets: Vec<Target>,
    snaps: Vec<Snap>,
    dest_dir: Option<PathBuf>,
    /// Final destination of each source (move/copy/rename) or of the new folder.
    dests: Vec<PathBuf>,
}

pub fn human_bytes(b: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

fn modified_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

/// Count files and bytes below a folder without following links.
fn walk(path: &Path, files: &mut u64, bytes: &mut u64, seen: &mut u64) -> bool {
    let Ok(rd) = std::fs::read_dir(path) else { return true };
    for e in rd.flatten() {
        *seen += 1;
        if *seen > MEASURE_CAP {
            return false;
        }
        let p = e.path();
        if paths::is_reparse(&p) {
            continue;
        }
        let Ok(m) = e.metadata() else { continue };
        if m.is_dir() {
            if !walk(&p, files, bytes, seen) {
                return false;
            }
        } else {
            *files += 1;
            *bytes += m.len();
        }
    }
    true
}

pub fn measure(path: &Path) -> Result<Target, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "That no longer exists, sir.".to_string())?;
    if meta.is_dir() {
        let (mut files, mut bytes, mut seen) = (0u64, 0u64, 0u64);
        let complete = walk(path, &mut files, &mut bytes, &mut seen);
        Ok(Target { path: path.to_path_buf(), is_dir: true, files, bytes, truncated: !complete })
    } else {
        Ok(Target { path: path.to_path_buf(), is_dir: false, files: 0, bytes: meta.len(), truncated: false })
    }
}

fn snap_of(t: &Target) -> Snap {
    let modified = std::fs::symlink_metadata(&t.path).map(|m| modified_secs(&m)).unwrap_or(0);
    Snap { path: t.path.clone(), is_dir: t.is_dir, bytes: t.bytes, files: t.files, modified }
}

/// `name (2).ext`, `name (3).ext`, ... : "keep both", never overwrite.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if std::fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for n in 2..1000 {
        let p = dir.join(format!("{stem} ({n}){ext}"));
        if std::fs::symlink_metadata(&p).is_err() {
            return p;
        }
    }
    dir.join(format!("{stem} (copy){ext}"))
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn root_of<'a>(p: &Path, roots: &'a [PathBuf]) -> Option<&'a PathBuf> {
    roots.iter().filter(|r| is_within(p, r)).max_by_key(|r| paths::key(r).len())
}

// ─── planning ─────────────────────────────────────────────────────────

fn blocked(audit_dir: &Path, ctx: &Ctx, tool: Tool, why: &str) -> Receipt {
    audit::record(
        audit_dir,
        &AuditEntry::new(ctx.now, ctx.phrase, tool.name(), tool.risk().as_str(), ctx.origin.as_str(), "deny", "blocked", why),
    );
    Receipt::blocked(why)
}

/// Resolve, measure and judge a request. `Err` is a finished (blocked) receipt; `Ok` carries the
/// verdict, which may still be Ask or Deny.
pub fn plan(ctx: &Ctx, req: &Request) -> Result<Plan, Receipt> {
    let tool = req.tool;
    let mut targets = Vec::new();
    for s in &req.sources {
        let c = paths::canonical(s).map_err(|e| blocked(ctx.dir, ctx, tool, &e.spoken()))?;
        targets.push(measure(&c).map_err(|e| blocked(ctx.dir, ctx, tool, &e))?);
    }
    let dest_dir = match &req.dest_dir {
        Some(d) => {
            let c = paths::canonical(d).map_err(|e| blocked(ctx.dir, ctx, tool, &e.spoken()))?;
            if !c.is_dir() {
                return Err(blocked(ctx.dir, ctx, tool, "That destination folder doesn't exist, sir."));
            }
            Some(c)
        }
        None => None,
    };
    // destination path(s)
    let mut dests = Vec::new();
    match tool {
        Tool::Move | Tool::Copy => {
            let Some(dd) = &dest_dir else {
                return Err(blocked(ctx.dir, ctx, tool, "I need a destination folder for that, sir."));
            };
            for t in &targets {
                if t.is_dir && is_within(dd, &t.path) {
                    return Err(blocked(ctx.dir, ctx, tool, "I can't put a folder inside itself, sir."));
                }
                if t.path.parent().map_or(false, |p| paths::key(p) == paths::key(dd)) && tool == Tool::Move {
                    return Err(blocked(ctx.dir, ctx, tool, "It's already in that folder, sir."));
                }
                dests.push(unique_path(dd, &file_name(&t.path)));
            }
        }
        Tool::Rename => {
            let name = req.new_name.clone().unwrap_or_default();
            check_name(&name).map_err(|e| blocked(ctx.dir, ctx, tool, &e.spoken()))?;
            let Some(t) = targets.first() else {
                return Err(blocked(ctx.dir, ctx, tool, "I don't know what to rename, sir."));
            };
            let parent = t.path.parent().unwrap_or(&t.path).to_path_buf();
            let new = parent.join(&name);
            if std::fs::symlink_metadata(&new).is_ok() && paths::key(&new) != paths::key(&t.path) {
                return Err(blocked(ctx.dir, ctx, tool, "Something with that name is already there, sir, so I won't overwrite it."));
            }
            dests.push(new);
        }
        Tool::CreateFolder => {
            let name = req.new_name.clone().unwrap_or_default();
            check_name(&name).map_err(|e| blocked(ctx.dir, ctx, tool, &e.spoken()))?;
            let Some(dd) = &dest_dir else {
                return Err(blocked(ctx.dir, ctx, tool, "I need a place to create it, sir."));
            };
            let new = dd.join(&name);
            if std::fs::symlink_metadata(&new).is_ok() {
                return Err(blocked(ctx.dir, ctx, tool, "A folder with that name is already there, sir."));
            }
            dests.push(new);
        }
        _ => {}
    }
    // the policy sees every path the action will touch: sources, destination folder, final names
    let mut extra: Vec<PathBuf> = Vec::new();
    if let Some(dd) = &dest_dir {
        extra.push(dd.clone());
    }
    extra.extend(dests.iter().cloned());
    let mut verdict = policy::evaluate(ctx.policy, ctx.env, ctx.roots, tool, &targets, extra.first().map(|p| p.as_path()), ctx.origin);
    if verdict.decision != Decision::Allow || extra.len() > 1 {
        // the same checks on the final names (a rename could land on a forbidden name)
        for d in extra.iter().skip(1) {
            if let Err(why) = policy::guard_path(d, ctx.env, ctx.policy, ctx.roots) {
                verdict = Verdict { decision: Decision::Deny(why), risk: verdict.risk };
                break;
            }
        }
    }
    let snaps = targets.iter().map(snap_of).collect();
    audit::record(
        ctx.dir,
        &AuditEntry::new(
            ctx.now,
            ctx.phrase,
            tool.name(),
            verdict.risk.as_str(),
            ctx.origin.as_str(),
            match &verdict.decision {
                Decision::Allow => "allow",
                Decision::Ask(_) => "ask",
                Decision::Deny(_) => "deny",
            },
            "planned",
            &targets.iter().map(|t| t.path.display().to_string()).collect::<Vec<_>>().join(" | "),
        ),
    );
    Ok(Plan { tool, verdict, phrase: ctx.phrase.to_string(), origin: ctx.origin, targets, snaps, dest_dir, dests })
}

fn describe(plan: &Plan) -> String {
    let names: Vec<String> = plan
        .targets
        .iter()
        .map(|t| {
            let n = file_name(&t.path);
            let n: String = n.chars().take(60).collect();
            if t.is_dir {
                format!("the folder {n}")
            } else {
                n
            }
        })
        .collect();
    match names.len() {
        0 => "nothing".into(),
        1 => names[0].clone(),
        2 | 3 => {
            let (last, head) = names.split_last().unwrap();
            format!("{} items: {} and {}", names.len(), head.join(", "), last)
        }
        n => format!("{n} items, including {} and {}", names[0], names[1]),
    }
}

fn totals(plan: &Plan) -> (u64, u64, bool) {
    let files: u64 = plan.targets.iter().map(|t| if t.is_dir { t.files } else { 1 }).sum();
    let bytes: u64 = plan.targets.iter().map(|t| t.bytes).sum();
    (files, bytes, plan.targets.iter().any(|t| t.truncated))
}

/// The spoken read-back and the sidebar card for a plan that needs a yes.
pub fn confirmation(plan: &Plan) -> (String, String) {
    let (files, bytes, trunc) = totals(plan);
    let folder = plan
        .targets
        .first()
        .and_then(|t| t.path.parent())
        .map(|p| file_name(p))
        .unwrap_or_default();
    let verb = match plan.tool {
        Tool::Delete => "Move to the trash",
        Tool::Move => "Move",
        Tool::Copy => "Copy",
        Tool::Rename => "Rename",
        Tool::CreateFolder => "Create",
        _ => "Do",
    };
    let why = match &plan.verdict.decision {
        Decision::Ask(w) => w.clone(),
        _ => String::new(),
    };
    let spoken = format!(
        "{verb} {} from {folder}, {}{}? Say yes to confirm.",
        describe(plan),
        human_bytes(bytes),
        if trunc { " or more" } else { "" }
    );
    let mut md = format!("## {verb}: please confirm\n\n");
    md.push_str(&format!("**Folder:** {folder}  \n**Items:** {}  \n**Size:** {}{}\n\n", plan.targets.len(), human_bytes(bytes), if trunc { "+" } else { "" }));
    for t in plan.targets.iter().take(25) {
        md.push_str(&format!(
            "- {}{} ({})\n",
            file_name(&t.path),
            if t.is_dir { "/" } else { "" },
            if t.is_dir { format!("{} files", t.files) } else { human_bytes(t.bytes) }
        ));
    }
    if plan.targets.len() > 25 {
        md.push_str(&format!("- ...and {} more\n", plan.targets.len() - 25));
    }
    let _ = files;
    md.push_str("\nNothing is permanently deleted. Say **undo that** to bring it back.\n");
    if !why.is_empty() {
        md.push_str(&format!("\n_Asking because: {why}._\n"));
    }
    (spoken, md)
}

// ─── execution ────────────────────────────────────────────────────────

fn copy_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    if paths::is_reparse(from) {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "refusing to copy a link"));
    }
    let meta = std::fs::symlink_metadata(from)?;
    if meta.is_dir() {
        std::fs::create_dir(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            copy_recursive(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

/// Same size and file count: how a copy is verified.
fn same_content(a: &Path, b: &Path) -> bool {
    match (measure(a), measure(b)) {
        (Ok(x), Ok(y)) => x.is_dir == y.is_dir && x.files == y.files && x.bytes == y.bytes,
        _ => false,
    }
}

fn quarantine_one(
    ctx: &Ctx,
    j: &mut Journal,
    batch: &str,
    path: &Path,
    target: &Target,
) -> Result<QEntry, String> {
    let root = root_of(path, ctx.roots).ok_or("That is outside my folders, sir.")?;
    let trash = root.join(TRASH_DIR).join(batch);
    std::fs::create_dir_all(&trash).map_err(|e| format!("I couldn't prepare the trash folder, sir ({e})."))?;
    let stored = unique_path(&trash, &file_name(path));
    std::fs::rename(path, &stored).map_err(|e| format!("I couldn't move it to the trash, sir ({e})."))?;
    let entry = QEntry {
        id: journal::new_id(ctx.now),
        batch: batch.to_string(),
        original: path.to_string_lossy().to_string(),
        stored: stored.to_string_lossy().to_string(),
        is_dir: target.is_dir,
        bytes: target.bytes,
        items: if target.is_dir { target.files.max(1) } else { 1 },
        at: ctx.now,
        restored: false,
    };
    j.quarantine.push(entry.clone());
    Ok(entry)
}

fn verified(spoken: String, detail: String) -> Receipt {
    Receipt { outcome: Outcome::Verified, spoken, detail }
}

fn done_audit(ctx: &Ctx, plan: &Plan, r: &Receipt) {
    audit::record(
        ctx.dir,
        &AuditEntry::new(
            ctx.now,
            ctx.phrase,
            plan.tool.name(),
            plan.verdict.risk.as_str(),
            ctx.origin.as_str(),
            "confirmed",
            r.outcome.as_str(),
            &r.spoken,
        ),
    );
}

/// Run a planned action. `confirmed` = the user said yes to the read-back (only meaningful when
/// the verdict is Ask). Re-checks everything first: the object must still be the one previewed.
pub fn execute(ctx: &Ctx, plan: &Plan, confirmed: bool) -> Receipt {
    let _guard = journal::LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match &plan.verdict.decision {
        Decision::Deny(w) => return blocked(ctx.dir, ctx, plan.tool, w),
        Decision::Ask(_) if !confirmed => return blocked(ctx.dir, ctx, plan.tool, "I need your yes first, sir."),
        _ => {}
    }
    // TOCTOU re-check: still there, unchanged, still allowed
    for (t, s) in plan.targets.iter().zip(&plan.snaps) {
        match measure(&t.path) {
            Ok(now) => {
                let cur = Snap {
                    path: now.path.clone(),
                    is_dir: now.is_dir,
                    bytes: now.bytes,
                    files: now.files,
                    modified: std::fs::symlink_metadata(&now.path).map(|m| modified_secs(&m)).unwrap_or(0),
                };
                if cur != *s {
                    return blocked(ctx.dir, ctx, plan.tool, "That changed since I checked it, sir, so I stopped. Ask me again.");
                }
            }
            Err(_) => return blocked(ctx.dir, ctx, plan.tool, "That's gone or changed since I checked, sir, so I stopped."),
        }
        if let Err(why) = policy::guard_path(&t.path, ctx.env, ctx.policy, ctx.roots) {
            return blocked(ctx.dir, ctx, plan.tool, &why);
        }
    }
    if let Some(dd) = &plan.dest_dir {
        if let Err(why) = policy::guard_dest(dd, ctx.env, ctx.policy, ctx.roots) {
            return blocked(ctx.dir, ctx, plan.tool, &why);
        }
    }
    let mut j = journal::load(ctx.dir);
    let batch_id = journal::new_id(ctx.now);
    let mut ops: Vec<Op> = Vec::new();
    let receipt = match plan.tool {
        Tool::Delete => {
            let mut moved = 0usize;
            let mut failure: Option<String> = None;
            for t in &plan.targets {
                match quarantine_one(ctx, &mut j, &batch_id, &t.path, t) {
                    Ok(e) => {
                        if std::fs::symlink_metadata(&t.path).is_ok() || std::fs::symlink_metadata(&e.stored).is_err() {
                            failure = Some("I moved it but couldn't confirm it, sir. Please check the folder.".into());
                            break;
                        }
                        ops.push(Op::Quarantined { entry: e.id });
                        moved += 1;
                    }
                    Err(e) => {
                        failure = Some(e);
                        break;
                    }
                }
            }
            let (_, bytes, _) = totals(plan);
            match failure {
                None => verified(
                    format!("Done, sir. {} moved to the trash, {}. Say undo that to bring {} back.", describe_count(moved), human_bytes(bytes), if moved == 1 { "it" } else { "them" }),
                    String::new(),
                ),
                Some(f) => Receipt {
                    outcome: if moved > 0 { Outcome::Unconfirmed } else { Outcome::Blocked },
                    spoken: format!("{f} {moved} of {} were moved.", plan.targets.len()),
                    detail: String::new(),
                },
            }
        }
        Tool::Move | Tool::Copy | Tool::Rename => {
            let mut done = 0usize;
            let mut failure: Option<String> = None;
            for (t, d) in plan.targets.iter().zip(&plan.dests) {
                let r: Result<(), String> = (|| {
                    if plan.tool == Tool::Copy {
                        copy_recursive(&t.path, d).map_err(|e| format!("The copy failed, sir ({e})."))?;
                        if !same_content(&t.path, d) {
                            let _ = std::fs::remove_dir_all(d).or_else(|_| std::fs::remove_file(d));
                            return Err("The copy didn't match the original, so I removed it, sir.".into());
                        }
                        ops.push(Op::Created { path: d.to_string_lossy().to_string() });
                        return Ok(());
                    }
                    match std::fs::rename(&t.path, d) {
                        Ok(()) => {
                            if std::fs::symlink_metadata(d).is_err() || std::fs::symlink_metadata(&t.path).is_ok() && plan.tool == Tool::Move {
                                return Err("I moved it but couldn't confirm it, sir.".into());
                            }
                            ops.push(Op::Moved { from: t.path.to_string_lossy().to_string(), to: d.to_string_lossy().to_string() });
                            Ok(())
                        }
                        Err(_) if plan.tool == Tool::Move => {
                            // another drive: copy, verify, then the original goes to the trash (still undoable)
                            copy_recursive(&t.path, d).map_err(|e| format!("The move failed, sir ({e})."))?;
                            if !same_content(&t.path, d) {
                                let _ = std::fs::remove_dir_all(d).or_else(|_| std::fs::remove_file(d));
                                return Err("The copy didn't match the original, so nothing was moved, sir.".into());
                            }
                            let e = quarantine_one(ctx, &mut j, &batch_id, &t.path, t)?;
                            ops.push(Op::Created { path: d.to_string_lossy().to_string() });
                            ops.push(Op::Quarantined { entry: e.id });
                            Ok(())
                        }
                        Err(e) => Err(format!("I couldn't do that, sir ({e}).")),
                    }
                })();
                match r {
                    Ok(()) => done += 1,
                    Err(e) => {
                        failure = Some(e);
                        break;
                    }
                }
            }
            let dest_name = plan.dest_dir.as_ref().map(|d| file_name(d)).unwrap_or_default();
            match failure {
                None => verified(
                    match plan.tool {
                        Tool::Copy => format!("Copied {} to {dest_name}, sir.", describe_count(done)),
                        Tool::Move => format!("Moved {} to {dest_name}, sir. Say undo that to put it back.", describe_count(done)),
                        _ => "Renamed, sir. Say undo that to change it back.".to_string(),
                    },
                    String::new(),
                ),
                Some(f) => Receipt {
                    outcome: if done > 0 { Outcome::Unconfirmed } else { Outcome::Blocked },
                    spoken: format!("{f} {done} of {} were done.", plan.targets.len()),
                    detail: String::new(),
                },
            }
        }
        Tool::CreateFolder => {
            let d = &plan.dests[0];
            match std::fs::create_dir(d) {
                Ok(()) if d.is_dir() => {
                    ops.push(Op::Created { path: d.to_string_lossy().to_string() });
                    verified(format!("Created the folder {}, sir.", file_name(d)), String::new())
                }
                Ok(()) => Receipt { outcome: Outcome::Unconfirmed, spoken: "I created it but couldn't confirm it, sir.".into(), detail: String::new() },
                Err(e) => Receipt::blocked(format!("I couldn't create it, sir ({e}).")),
            }
        }
        _ => Receipt::blocked("That isn't something I can run that way, sir."),
    };
    if !ops.is_empty() {
        j.push_batch(Batch { id: batch_id, tool: plan.tool.name().into(), at: ctx.now, phrase: ctx.phrase.chars().take(200).collect(), ops, undone: false });
    }
    if let Err(e) = journal::save(ctx.dir, &j) {
        tracing::warn!("action kernel: journal save failed: {e}");
    }
    done_audit(ctx, plan, &receipt);
    receipt
}

fn describe_count(n: usize) -> String {
    if n == 1 {
        "1 item".into()
    } else {
        format!("{n} items")
    }
}

// ─── undo and restore ─────────────────────────────────────────────────

fn restore_entry_in(ctx: &Ctx, j: &mut Journal, id: &str) -> Result<PathBuf, String> {
    let Some(e) = j.entry(id).cloned() else { return Err("I don't have that in the trash, sir.".into()) };
    if e.restored {
        return Err("That was already restored, sir.".into());
    }
    let stored = PathBuf::from(&e.stored);
    if std::fs::symlink_metadata(&stored).is_err() {
        return Err("That's no longer in the trash, sir.".into());
    }
    let stored_c = paths::canonical(&stored).map_err(|e| e.spoken())?;
    if !paths::has_component(&stored_c, &[TRASH_DIR]) || !ctx.roots.iter().any(|r| is_within(&stored_c, r)) {
        return Err("That doesn't look like something I put in the trash, so I won't touch it, sir.".into());
    }
    let original = PathBuf::from(&e.original);
    let parent = original.parent().ok_or("I don't know where that came from, sir.")?;
    if !parent.is_dir() {
        return Err("The folder it came from is gone, sir.".into());
    }
    let parent_c = paths::canonical(parent).map_err(|e| e.spoken())?;
    policy::guard_dest(&parent_c, ctx.env, ctx.policy, ctx.roots)?;
    let target = unique_path(&parent_c, &file_name(&original));
    std::fs::rename(&stored, &target).map_err(|e| format!("I couldn't put it back, sir ({e})."))?;
    if std::fs::symlink_metadata(&target).is_err() {
        return Err("I moved it but couldn't confirm it, sir.".into());
    }
    if let Some(m) = j.quarantine.iter_mut().find(|q| q.id == id) {
        m.restored = true;
    }
    Ok(target)
}

/// Bring one trashed item back (Hub button).
pub fn restore_entry(ctx: &Ctx, id: &str) -> Receipt {
    let _guard = journal::LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut j = journal::load(ctx.dir);
    let r = match restore_entry_in(ctx, &mut j, id) {
        Ok(p) => verified(format!("Restored {}, sir.", file_name(&p)), String::new()),
        Err(e) => Receipt::blocked(e),
    };
    let _ = journal::save(ctx.dir, &j);
    audit::record(
        ctx.dir,
        &AuditEntry::new(ctx.now, ctx.phrase, "restore", Risk::Reversible.as_str(), ctx.origin.as_str(), "allow", r.outcome.as_str(), &r.spoken),
    );
    r
}

/// "Undo that": reverse the newest action that has not been undone.
pub fn undo_last(ctx: &Ctx) -> Receipt {
    let _guard = journal::LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut j = journal::load(ctx.dir);
    let Some(batch) = j.last_undoable().cloned() else {
        return Receipt::blocked("There's nothing to undo, sir.");
    };
    let mut undone = 0usize;
    let mut problems: Vec<String> = Vec::new();
    for op in batch.ops.iter().rev() {
        let r: Result<(), String> = match op {
            Op::Quarantined { entry } => restore_entry_in(ctx, &mut j, entry).map(|_| ()),
            Op::Moved { from, to } => (|| {
                let (from, to) = (PathBuf::from(from), PathBuf::from(to));
                if std::fs::symlink_metadata(&to).is_err() {
                    return Err("The moved item isn't where I left it, sir.".to_string());
                }
                let back = if std::fs::symlink_metadata(&from).is_err() { from.clone() } else { unique_path(from.parent().unwrap_or(&from), &file_name(&from)) };
                let parent = back.parent().ok_or("No folder to return it to, sir.")?;
                policy::guard_dest(&paths::canonical(parent).map_err(|e| e.spoken())?, ctx.env, ctx.policy, ctx.roots)?;
                policy::guard_path(&paths::canonical(&to).map_err(|e| e.spoken())?, ctx.env, ctx.policy, ctx.roots)?;
                std::fs::rename(&to, &back).map_err(|e| format!("I couldn't move it back, sir ({e})."))
            })(),
            Op::Created { path } => (|| {
                let p = PathBuf::from(path);
                if std::fs::symlink_metadata(&p).is_err() {
                    return Ok(()); // already gone
                }
                let c = paths::canonical(&p).map_err(|e| e.spoken())?;
                policy::guard_path(&c, ctx.env, ctx.policy, ctx.roots)?;
                if c.is_dir() && std::fs::read_dir(&c).map(|mut r| r.next().is_none()).unwrap_or(false) {
                    return std::fs::remove_dir(&c).map_err(|e| format!("I couldn't remove the empty folder, sir ({e})."));
                }
                let t = measure(&c)?;
                quarantine_one(ctx, &mut j, &batch.id, &c, &t).map(|_| ())
            })(),
        };
        match r {
            Ok(()) => undone += 1,
            Err(e) => problems.push(e),
        }
    }
    if problems.is_empty() {
        if let Some(b) = j.batches.iter_mut().find(|b| b.id == batch.id) {
            b.undone = true;
        }
    }
    let _ = journal::save(ctx.dir, &j);
    let receipt = if problems.is_empty() {
        verified(format!("Done, sir. I undid the {} ({}).", batch.tool, describe_count(undone)), String::new())
    } else {
        Receipt {
            outcome: if undone > 0 { Outcome::Unconfirmed } else { Outcome::Blocked },
            spoken: format!("I could only undo part of that, sir. {}", problems[0]),
            detail: problems.join("\n"),
        }
    };
    audit::record(
        ctx.dir,
        &AuditEntry::new(ctx.now, ctx.phrase, "undo", Risk::Reversible.as_str(), ctx.origin.as_str(), "allow", receipt.outcome.as_str(), &receipt.spoken),
    );
    receipt
}

/// Remove trash older than the policy's retention. The only permanent deletion in the kernel, only
/// ever inside a `.nexus-trash` folder under an allowed root, always audited. Returns how many.
pub fn purge_expired(ctx: &Ctx) -> usize {
    let _guard = journal::LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut j = journal::load(ctx.dir);
    let due = j.purge_due(ctx.now, ctx.policy.quarantine_days);
    let mut removed = 0usize;
    for e in &due {
        let p = PathBuf::from(&e.stored);
        let safe = paths::canonical(&p)
            .map(|c| paths::has_component(&c, &[TRASH_DIR]) && ctx.roots.iter().any(|r| is_within(&c, r)) && !paths::is_reparse(&c))
            .unwrap_or(false);
        if std::fs::symlink_metadata(&p).is_err() {
            j.quarantine.retain(|q| q.id != e.id); // already gone
            continue;
        }
        if !safe {
            continue;
        }
        let ok = if p.is_dir() { std::fs::remove_dir_all(&p).is_ok() } else { std::fs::remove_file(&p).is_ok() };
        if ok {
            j.quarantine.retain(|q| q.id != e.id);
            removed += 1;
            audit::record(
                ctx.dir,
                &AuditEntry::new(ctx.now, "", "purge", Risk::Destructive.as_str(), "system", "allow", "verified", &format!("{} (trashed {} days ago)", e.original, (ctx.now - e.at) / 86_400)),
            );
        }
    }
    let _ = journal::save(ctx.dir, &j);
    removed
}

// ─── reading ──────────────────────────────────────────────────────────

/// Markdown table of a folder's newest entries (Read: instant, allowed inside the roots).
pub fn list_card(ctx: &Ctx, folder: &Path, query: &super::resolve::Query) -> Result<(String, String), Receipt> {
    let c = paths::canonical(folder).map_err(|e| Receipt::blocked(e.spoken()))?;
    policy::guard_dest(&c, ctx.env, ctx.policy, ctx.roots).map_err(Receipt::blocked)?;
    let all = super::resolve::list_folder(&c);
    let shown: Vec<_> = all.iter().filter(|e| super::resolve::matches_query(e, query)).take(25).collect();
    let total = all.iter().filter(|e| super::resolve::matches_query(e, query)).count();
    let name = file_name(&c);
    let mut md = format!("## {name}\n\n| Name | Size |\n|---|---|\n");
    for e in &shown {
        md.push_str(&format!("| {}{} | {} |\n", e.name, if e.is_dir { "/" } else { "" }, if e.is_dir { "folder".into() } else { human_bytes(e.bytes) }));
    }
    if total > shown.len() {
        md.push_str(&format!("\n...and {} more.\n", total - shown.len()));
    }
    audit::record(ctx.dir, &AuditEntry::new(ctx.now, ctx.phrase, "list", Risk::Read.as_str(), ctx.origin.as_str(), "allow", "verified", &name));
    let spoken = match total {
        0 => format!("{name} has nothing matching, sir."),
        1 => format!("There's 1 item in {name}, sir. The details are in the sidebar."),
        n => format!("There are {n} items in {name}, sir. The list is in the sidebar."),
    };
    Ok((spoken, md))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_kernel::paths::test_dir;
    use crate::action_kernel::policy::test_env;

    struct World {
        root: PathBuf,
        env: PolicyEnv,
        policy: Policy,
        roots: Vec<PathBuf>,
        data: PathBuf,
    }

    fn world() -> World {
        let root = test_dir("files");
        let (env, policy, roots) = test_env(&root);
        let data = env.data_dir.clone();
        World { root, env, policy, roots, data }
    }

    fn ctx<'a>(w: &'a World, origin: Origin, now: i64) -> Ctx<'a> {
        Ctx { dir: &w.data, policy: &w.policy, env: &w.env, roots: &w.roots, now, phrase: "test phrase", origin }
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        paths::canonical(&p).unwrap()
    }

    fn req(tool: Tool, sources: Vec<PathBuf>) -> Request {
        Request { tool, sources, dest_dir: None, new_name: None }
    }

    #[test]
    fn delete_asks_then_moves_to_quarantine_and_undo_restores_it() {
        let w = world();
        let docs = &w.roots[0];
        let f = write(docs, "report.pdf", "important");
        let c = ctx(&w, Origin::UserVoice, 1000);
        let p = plan(&c, &req(Tool::Delete, vec![f.clone()])).unwrap();
        assert!(matches!(p.verdict.decision, Decision::Ask(_)));
        // never runs without the yes
        let r = execute(&c, &p, false);
        assert_eq!(r.outcome, Outcome::Blocked);
        assert!(f.exists());
        let r = execute(&c, &p, true);
        assert_eq!(r.outcome, Outcome::Verified, "{}", r.spoken);
        assert!(!f.exists());
        let j = journal::load(&w.data);
        let q = &j.quarantine[0];
        assert!(Path::new(&q.stored).exists() && q.stored.contains(TRASH_DIR));
        // undo
        let u = undo_last(&ctx(&w, Origin::UserVoice, 1010));
        assert_eq!(u.outcome, Outcome::Verified, "{}", u.spoken);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "important");
        assert!(journal::load(&w.data).quarantine[0].restored);
        assert_eq!(undo_last(&ctx(&w, Origin::UserVoice, 1011)).outcome, Outcome::Blocked, "nothing left to undo");
    }

    #[test]
    fn restoring_onto_an_occupied_name_keeps_both() {
        let w = world();
        let docs = &w.roots[0];
        let f = write(docs, "a.txt", "one");
        let c = ctx(&w, Origin::UserVoice, 5);
        let p = plan(&c, &req(Tool::Delete, vec![f.clone()])).unwrap();
        assert_eq!(execute(&c, &p, true).outcome, Outcome::Verified);
        write(docs, "a.txt", "two");
        let u = undo_last(&c);
        assert_eq!(u.outcome, Outcome::Verified);
        assert_eq!(std::fs::read_to_string(docs.join("a.txt")).unwrap(), "two");
        assert_eq!(std::fs::read_to_string(docs.join("a (2).txt")).unwrap(), "one");
    }

    #[test]
    fn a_file_that_changes_after_the_preview_is_not_deleted() {
        let w = world();
        let f = write(&w.roots[0], "a.txt", "one");
        let c = ctx(&w, Origin::UserVoice, 5);
        let p = plan(&c, &req(Tool::Delete, vec![f.clone()])).unwrap();
        std::fs::write(&f, "one plus a lot more").unwrap();
        let r = execute(&c, &p, true);
        assert_eq!(r.outcome, Outcome::Blocked);
        assert!(f.exists());
        // gone entirely also blocks
        let p = plan(&c, &req(Tool::Delete, vec![f.clone()])).unwrap();
        std::fs::remove_file(&f).unwrap();
        assert_eq!(execute(&c, &p, true).outcome, Outcome::Blocked);
    }

    #[test]
    fn folders_are_quarantined_whole_and_measured() {
        let w = world();
        let d = w.roots[0].join("old project");
        std::fs::create_dir_all(d.join("src")).unwrap();
        write(&d, "a.txt", "aaa");
        write(&d.join("src"), "b.txt", "bbbb");
        let c = ctx(&w, Origin::UserVoice, 7);
        let p = plan(&c, &req(Tool::Delete, vec![d.clone()])).unwrap();
        let (spoken, md) = confirmation(&p);
        assert!(spoken.contains("old project") && spoken.contains("yes"), "{spoken}");
        assert!(md.contains("2 files"));
        assert_eq!(execute(&c, &p, true).outcome, Outcome::Verified);
        assert!(!d.exists());
        assert_eq!(undo_last(&c).outcome, Outcome::Verified);
        assert_eq!(std::fs::read_to_string(d.join("src").join("b.txt")).unwrap(), "bbbb");
    }

    #[test]
    fn derived_origin_cannot_delete_even_with_a_yes() {
        let w = world();
        let f = write(&w.roots[0], "a.txt", "x");
        let c = ctx(&w, Origin::Derived, 5);
        let p = plan(&c, &req(Tool::Delete, vec![f.clone()])).unwrap();
        assert!(matches!(p.verdict.decision, Decision::Deny(_)));
        assert_eq!(execute(&c, &p, true).outcome, Outcome::Blocked);
        assert!(f.exists());
    }

    #[test]
    fn move_copy_rename_never_overwrite_and_undo_round_trips() {
        let w = world();
        let docs = &w.roots[0];
        let dest = docs.join("archive");
        std::fs::create_dir_all(&dest).unwrap();
        let dest = paths::canonical(&dest).unwrap();
        let f = write(docs, "a.txt", "alpha");
        write(&dest, "a.txt", "already here");
        let c = ctx(&w, Origin::UserVoice, 10);
        // copy keeps both
        let p = plan(&c, &Request { tool: Tool::Copy, sources: vec![f.clone()], dest_dir: Some(dest.clone()), new_name: None }).unwrap();
        assert_eq!(p.verdict.decision, Decision::Allow);
        assert_eq!(execute(&c, &p, false).outcome, Outcome::Verified);
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "already here");
        assert_eq!(std::fs::read_to_string(dest.join("a (2).txt")).unwrap(), "alpha");
        assert!(f.exists());
        // undo the copy: the copy goes to the trash, the original stays
        assert_eq!(undo_last(&c).outcome, Outcome::Verified);
        assert!(!dest.join("a (2).txt").exists() && f.exists());
        // move, then undo
        let p = plan(&c, &Request { tool: Tool::Move, sources: vec![f.clone()], dest_dir: Some(dest.clone()), new_name: None }).unwrap();
        assert_eq!(execute(&c, &p, false).outcome, Outcome::Verified);
        assert!(!f.exists() && dest.join("a (2).txt").exists());
        assert_eq!(undo_last(&c).outcome, Outcome::Verified);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "alpha");
        // rename refuses to overwrite and refuses bad names
        write(docs, "b.txt", "bee");
        let r = plan(&c, &Request { tool: Tool::Rename, sources: vec![f.clone()], dest_dir: None, new_name: Some("b.txt".into()) });
        assert!(r.is_err());
        for bad in ["..", "x:y", "con", "name."] {
            assert!(plan(&c, &Request { tool: Tool::Rename, sources: vec![f.clone()], dest_dir: None, new_name: Some(bad.into()) }).is_err(), "{bad}");
        }
        let p = plan(&c, &Request { tool: Tool::Rename, sources: vec![f.clone()], dest_dir: None, new_name: Some("alpha.txt".into()) }).unwrap();
        assert_eq!(execute(&c, &p, false).outcome, Outcome::Verified);
        assert!(!f.exists() && docs.join("alpha.txt").exists());
        assert_eq!(undo_last(&c).outcome, Outcome::Verified);
        assert!(f.exists());
    }

    #[test]
    fn a_folder_cannot_move_into_itself_or_to_where_it_already_is() {
        let w = world();
        let d = w.roots[0].join("proj");
        std::fs::create_dir_all(d.join("inner")).unwrap();
        let d = paths::canonical(&d).unwrap();
        let c = ctx(&w, Origin::UserVoice, 1);
        let inner = d.join("inner");
        assert!(plan(&c, &Request { tool: Tool::Move, sources: vec![d.clone()], dest_dir: Some(inner), new_name: None }).is_err());
        assert!(plan(&c, &Request { tool: Tool::Move, sources: vec![d.clone()], dest_dir: Some(w.roots[0].clone()), new_name: None }).is_err());
    }

    #[test]
    fn create_folder_and_undo_removes_only_the_empty_folder() {
        let w = world();
        let c = ctx(&w, Origin::UserVoice, 3);
        let r = Request { tool: Tool::CreateFolder, sources: vec![], dest_dir: Some(w.roots[0].clone()), new_name: Some("taxes".into()) };
        let p = plan(&c, &r).unwrap();
        assert_eq!(p.verdict.decision, Decision::Allow);
        assert_eq!(execute(&c, &p, false).outcome, Outcome::Verified);
        let made = w.roots[0].join("taxes");
        assert!(made.is_dir());
        assert!(plan(&c, &r).is_err(), "second create refuses");
        // fill it: undo then moves it to the trash instead of losing the file
        write(&made, "x.txt", "keep me");
        assert_eq!(undo_last(&c).outcome, Outcome::Verified);
        assert!(!made.exists());
        let j = journal::load(&w.data);
        assert!(j.quarantine.iter().any(|q| q.original.ends_with("taxes")));
    }

    #[test]
    fn forbidden_things_never_reach_execute() {
        let w = world();
        let c = ctx(&w, Origin::UserVoice, 1);
        for bad in [w.root.join("Windows"), w.env.home.clone(), w.roots[0].clone(), w.env.data_dir.clone()] {
            std::fs::create_dir_all(&bad).unwrap();
            let p = plan(&c, &req(Tool::Delete, vec![bad.clone()])).unwrap();
            assert!(matches!(p.verdict.decision, Decision::Deny(_)), "{bad:?}");
            assert_eq!(execute(&c, &p, true).outcome, Outcome::Blocked);
            assert!(bad.exists());
        }
        // a path spelled through ".." from inside the root resolves to the real forbidden place
        let sneaky = w.roots[0].join("..").join("..");
        let p = plan(&c, &req(Tool::Delete, vec![sneaky])).unwrap();
        assert!(matches!(p.verdict.decision, Decision::Deny(_)));
    }

    #[test]
    fn a_tampered_manifest_can_not_make_restore_move_files_out_of_arbitrary_places() {
        let w = world();
        let victim = write(&w.roots[0], "innocent.txt", "mine");
        let mut j = journal::load(&w.data);
        j.quarantine.push(QEntry {
            id: "evil".into(),
            batch: "b".into(),
            original: w.roots[0].join("planted.txt").to_string_lossy().to_string(),
            stored: victim.to_string_lossy().to_string(),
            is_dir: false,
            bytes: 4,
            items: 1,
            at: 1,
            restored: false,
        });
        journal::save(&w.data, &j).unwrap();
        let r = restore_entry(&ctx(&w, Origin::UserVoice, 5), "evil");
        assert_eq!(r.outcome, Outcome::Blocked, "{}", r.spoken);
        assert!(victim.exists() && !w.roots[0].join("planted.txt").exists());
    }

    #[test]
    fn purge_removes_only_old_trash_inside_a_trash_folder() {
        let w = world();
        let f = write(&w.roots[0], "old.txt", "x");
        let c = ctx(&w, Origin::UserVoice, 100);
        let p = plan(&c, &req(Tool::Delete, vec![f.clone()])).unwrap();
        assert_eq!(execute(&c, &p, true).outcome, Outcome::Verified);
        // too early: nothing happens
        assert_eq!(purge_expired(&ctx(&w, Origin::UserVoice, 100 + 86_400)), 0);
        // a tampered manifest entry pointing outside the trash is never deleted
        let victim = write(&w.roots[0], "precious.txt", "keep");
        let mut j = journal::load(&w.data);
        j.quarantine.push(QEntry { id: "evil".into(), batch: "b".into(), original: String::new(), stored: victim.to_string_lossy().to_string(), is_dir: false, bytes: 1, items: 1, at: 0, restored: false });
        journal::save(&w.data, &j).unwrap();
        let removed = purge_expired(&ctx(&w, Origin::UserVoice, 100 + 40 * 86_400));
        assert_eq!(removed, 1, "only the real trashed item");
        assert!(victim.exists(), "files outside .nexus-trash are never purged");
        let j = journal::load(&w.data);
        assert!(j.quarantine.iter().all(|q| q.id == "evil"));
    }

    #[test]
    fn every_decision_is_audited() {
        let w = world();
        let f = write(&w.roots[0], "a.txt", "x");
        let c = ctx(&w, Origin::UserVoice, 9);
        let p = plan(&c, &req(Tool::Delete, vec![f])).unwrap();
        execute(&c, &p, true);
        let a = audit::tail(&w.data, 10);
        assert!(a.iter().any(|e| e.decision == "ask" && e.tool == "delete" && e.outcome == "planned"));
        assert!(a.iter().any(|e| e.decision == "confirmed" && e.outcome == "verified"));
    }

    #[test]
    fn list_card_reads_inside_the_roots_only() {
        let w = world();
        write(&w.roots[0], "a.pdf", "x");
        write(&w.roots[0], "b.txt", "y");
        let c = ctx(&w, Origin::UserVoice, 1);
        let (spoken, md) = list_card(&c, &w.roots[0], &super::super::resolve::parse_object("pdfs")).unwrap();
        assert!(spoken.contains("1 item") && md.contains("a.pdf") && !md.contains("b.txt"));
        assert!(list_card(&c, &w.root, &Default::default()).is_err());
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human_bytes(10), "10 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MB");
    }
}
