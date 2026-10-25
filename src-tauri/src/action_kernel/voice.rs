//! Voice layer for file actions (docs/features/140, P3): phrase -> intent, intent -> a reply that is
//! spoken, shown or held for confirmation. Pure of audio and windows so it is fully testable.
//!
//! Safety stance: a phrase that clearly asks to remove or wipe things is *always* claimed here, even
//! when it is too broad ("delete everything", "wipe my c drive"), so it is refused locally and never
//! reaches a cloud chat model that might pretend to do it. A phrase that only looks like a file
//! command by accident ("delete that message", "remove the background") is left alone.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;
use regex::Regex;

use super::files::{self, Ctx, Plan, Request};
use super::policy::Decision;
use super::resolve::{self, Resolved};
use super::Tool;
use crate::intent_parser::ParsedIntent;

const KNOWN_ALIASES: &[&str] = &[
    "downloads", "download", "desktop", "documents", "document", "docs", "pictures", "photos", "videos", "music",
];

/// Names of folders the user added in the Hub ("projects"), so "in the projects folder" parses.
static EXTRA_ALIASES: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn set_extra_aliases(roots: &[String]) {
    let names = roots
        .iter()
        .filter_map(|r| Path::new(r).file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .filter(|n| !n.is_empty())
        .collect();
    *EXTRA_ALIASES.lock().unwrap_or_else(|e| e.into_inner()) = names;
}

fn is_alias(s: &str) -> bool {
    let s = s.trim();
    KNOWN_ALIASES.contains(&s) || EXTRA_ALIASES.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|a| a == s)
}

fn clean(t: &str) -> String {
    let mut s = t.to_lowercase();
    s = s.replace(['\u{2019}', '`'], "'");
    s = s.chars().map(|c| if c.is_alphanumeric() || c == '\'' || c == '.' || c == ' ' || c == '-' { c } else { ' ' }).collect();
    let mut s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let before = s.clone();
        for p in ["nexus ", "please ", "can you ", "could you ", "would you ", "i want you to ", "i want to ", "go ahead and ", "hey "] {
            if let Some(r) = s.strip_prefix(p) {
                s = r.to_string();
            }
        }
        if s == before {
            break;
        }
    }
    s.trim_end_matches(" please").trim_end_matches('.').to_string()
}

/// Words that mean the "delete" is about something else (handled elsewhere or not a file at all).
const NOT_FILES: &[&str] = &[
    "message", "messages", "text", "texts", "email", "emails", "mail", "chat", "chats", "tab", "tabs", "window", "account",
    "memory", "memories", "reminder", "reminders", "event", "events", "contact", "contacts", "history", "cache", "cookies",
    "app", "apps", "program", "programs", "background", "notification", "notifications", "word", "words", "letter",
    "line", "lines", "sentence", "character", "characters", "timetable", "alarm", "pr", "branch", "repo", "repository",
    "playlist", "song", "queue", "label", "filter", "rule", "slot", "slots", "watch", "block", "selection", "layer",
];

const WIPE_TARGETS: &[&str] = &[
    "drive", "disk", "laptop", "pc", "computer", "hard drive", "system", "windows", "c drive", "d drive", "everything", "all my files",
    "all files", "all the files", "all my data", "my data", "my files", "my documents", "my stuff", "the whole", "storage",
];

static RE_UNDO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:undo (?:that|it|this|the last (?:action|delete|move|copy|rename|change|thing)|the (?:delete|move|copy|rename|deletion))|(?:bring|put|get) (?:it|them|that|those) back|restore (?:it|them|that|those|the last (?:deleted )?(?:file|files|thing))|take that back)$").unwrap()
});
static RE_DELETE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:delete|remove|trash|bin|erase|wipe(?: out)?|destroy|shred|get rid of|throw away|throw out|clear out|format|empty) (.+)$").unwrap()
});
static RE_MOVE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(move|copy|put|transfer) (.+?) (?:in|into|to|inside|onto) (.+)$").unwrap());
static RE_RENAME: Lazy<Regex> = Lazy::new(|| Regex::new(r"^rename (.+?) (?:to|as) (.+)$").unwrap());
static RE_CREATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:create|make|add|new) (?:me )?(?:a |an )?(?:new )?(?:folder|directory)(?: (?:called|named|with the name))? (.+)$").unwrap()
});
static RE_LIST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:list|what is in|what's in|whats in|what do i have in|what files are in|how many files are in|show me what is in|show me what's in|show me the files in|show the files in|list the files in) (.+)$").unwrap()
});
static RE_FIND: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:find|look for|search for|where is|where's|locate) (.+)$").unwrap());

fn strip_articles(s: &str) -> String {
    let mut out = s.trim().to_string();
    for p in ["the ", "my ", "a ", "an ", "that ", "this ", "those ", "these "] {
        while let Some(r) = out.strip_prefix(p) {
            out = r.to_string();
        }
    }
    out
}

/// Split "<object> from/in/on <folder alias>" when (and only when) the tail is a known folder.
fn split_folder(s: &str) -> (String, Option<String>) {
    for sep in [" from ", " in ", " on ", " inside ", " out of ", " under "] {
        if let Some((head, tail)) = s.rsplit_once(sep) {
            let t = strip_articles(tail).trim_end_matches(" folder").trim_end_matches(" directory").to_string();
            if is_alias(&t) {
                return (head.trim().to_string(), Some(t));
            }
        }
    }
    (s.trim().to_string(), None)
}

fn has_file_cue(obj: &str) -> bool {
    let q = resolve::parse_object(obj);
    if !q.exts.is_empty() || q.name_prefix.is_some() || q.dirs_only {
        return true;
    }
    let words: Vec<&str> = obj.split_whitespace().collect();
    words.iter().any(|w| matches!(*w, "file" | "files" | "folder" | "folders" | "download" | "downloads" | "screenshot" | "screenshots"))
        || split_folder(obj).1.is_some()
}

fn wipes_everything(obj: &str) -> bool {
    let raw = obj.trim();
    let o = strip_articles(raw);
    let bare = o.trim_end_matches(" folder").trim_end_matches(" directory").to_string();
    is_alias(&bare)
        || o.starts_with("everything")
        || o.starts_with("contents of")
        || o.starts_with("the contents of")
        || WIPE_TARGETS.iter().any(|t| {
            raw == *t || o == *t || o.starts_with(&format!("{t} ")) || o.ends_with(&format!(" {t}")) || o.contains(&format!("all my {t}"))
        })
}

/// The voice grammar. `None` = not a file command (the rest of the parser chain gets the phrase).
pub fn parse(transcript: &str) -> Option<ParsedIntent> {
    let t = clean(transcript);
    if t.is_empty() {
        return None;
    }
    if RE_UNDO.is_match(&t) {
        return Some(ParsedIntent::FileUndo);
    }
    if let Some(c) = RE_DELETE.captures(&t) {
        let obj = c[1].to_string();
        let verb_wipe = t.starts_with("wipe") || t.starts_with("format") || t.starts_with("erase") || t.starts_with("destroy") || t.starts_with("shred");
        let words: Vec<&str> = obj.split_whitespace().collect();
        if words.iter().any(|w| NOT_FILES.contains(w)) && !wipes_everything(&obj) {
            return None;
        }
        if wipes_everything(&obj) || (verb_wipe && !has_file_cue(&obj) && words.len() <= 4) {
            // too broad: claimed so it is refused here, never sent to a chat model
            return Some(ParsedIntent::FileAction { verb: "delete".into(), object: "everything".into(), folder: None, dest: None });
        }
        if !has_file_cue(&obj) {
            return None;
        }
        let (object, folder) = split_folder(&obj);
        return Some(ParsedIntent::FileAction { verb: "delete".into(), object: strip_articles(&object), folder, dest: None });
    }
    if let Some(c) = RE_RENAME.captures(&t) {
        let obj = c[1].to_string();
        let words: Vec<&str> = obj.split_whitespace().collect();
        if words.iter().any(|w| NOT_FILES.contains(w) || matches!(*w, "laptop" | "pc" | "computer" | "group" | "ghost" | "mode")) {
            return None;
        }
        let (object, folder) = split_folder(&obj);
        let new_name = strip_articles(&c[2]).trim_end_matches(" file").trim_end_matches(" folder").to_string();
        if new_name.is_empty() {
            return None;
        }
        return Some(ParsedIntent::FileAction { verb: "rename".into(), object: strip_articles(&object), folder, dest: Some(new_name) });
    }
    if let Some(c) = RE_MOVE.captures(&t) {
        let (verb, obj, dest) = (c[1].to_string(), c[2].to_string(), c[3].to_string());
        let words: Vec<&str> = obj.split_whitespace().collect();
        if words.iter().any(|w| NOT_FILES.contains(w)) {
            return None;
        }
        let dest_t = strip_articles(&dest);
        let dest_ok = is_alias(dest_t.trim_end_matches(" folder")) || dest_t.contains(" folder in ") || dest_t.contains(" folder inside ") || dest_t.contains(" folder on ");
        if !dest_ok || !has_file_cue(&obj) {
            return None;
        }
        let verb = if verb == "copy" { "copy" } else { "move" };
        let (object, folder) = split_folder(&obj);
        return Some(ParsedIntent::FileAction { verb: verb.into(), object: strip_articles(&object), folder, dest: Some(dest_t) });
    }
    if let Some(c) = RE_CREATE.captures(&t) {
        let rest = c[1].to_string();
        let (name, folder) = split_folder(&rest);
        let name = strip_articles(&name);
        if name.is_empty() {
            return None;
        }
        return Some(ParsedIntent::FileAction { verb: "create_folder".into(), object: name, folder, dest: None });
    }
    if let Some(c) = RE_LIST.captures(&t) {
        let rest = c[1].to_string();
        let (object, folder) = split_folder(&rest);
        // "what's in downloads": the whole tail is the folder
        if folder.is_none() {
            let f = strip_articles(&rest).trim_end_matches(" folder").to_string();
            if is_alias(&f) {
                return Some(ParsedIntent::FileAction { verb: "list".into(), object: String::new(), folder: Some(f), dest: None });
            }
            return None;
        }
        return Some(ParsedIntent::FileAction { verb: "list".into(), object: strip_articles(&object), folder, dest: None });
    }
    if let Some(c) = RE_FIND.captures(&t) {
        let (object, folder) = split_folder(&c[1]);
        // "search for X" alone is a web search: only claimed when a known folder is named
        if folder.is_some() && !object.is_empty() {
            return Some(ParsedIntent::FileAction { verb: "find".into(), object: strip_articles(&object), folder, dest: None });
        }
    }
    None
}

// ─── handling ─────────────────────────────────────────────────────────

pub enum Reply {
    Say(String),
    /// Spoken line plus a sidebar card.
    Card { spoken: String, title: String, markdown: String },
    /// A plan waiting for a yes: read-back, sidebar preview, the held plan.
    Confirm { spoken: String, markdown: String, plan: Box<Plan> },
}

fn say(s: impl Into<String>) -> Reply {
    Reply::Say(s.into())
}

fn names_of(c: &[resolve::Candidate]) -> String {
    let n: Vec<String> = c.iter().map(|c| c.name.chars().take(50).collect()).collect();
    match n.len() {
        0 => String::new(),
        1 => n[0].clone(),
        _ => format!("{} and {}", n[..n.len() - 1].join(", "), n[n.len() - 1]),
    }
}

/// "taxes folder in documents" / "documents" -> a real destination folder.
fn resolve_dest(spoken: &str, ctx: &Ctx) -> Result<PathBuf, String> {
    if let Some(p) = resolve::resolve_folder(spoken, ctx.policy, ctx.roots) {
        return Ok(p);
    }
    for sep in [" folder in ", " folder inside ", " folder on "] {
        if let Some((name, parent)) = spoken.split_once(sep) {
            let Some(parent_dir) = resolve::resolve_folder(parent, ctx.policy, ctx.roots) else {
                return Err(format!("I don't know the folder {parent}, sir."));
            };
            let q = resolve::Query { tokens: strip_articles(name).split_whitespace().map(str::to_string).collect(), dirs_only: true, ..Default::default() };
            let hits: Vec<_> = resolve::list_folder(&parent_dir).into_iter().filter(|c| resolve::matches_query(c, &q)).collect();
            return match hits.len() {
                1 => Ok(hits[0].path.clone()),
                0 => Err(format!("I don't see a folder called {name} in {parent}, sir. Say make a folder called {name} first.")),
                _ => Err(format!("More than one folder fits {name}, sir: {}. Say the exact name.", names_of(&hits.into_iter().take(4).collect::<Vec<_>>()))),
            };
        }
    }
    Err(format!("I don't know a folder called {spoken} that I'm allowed to use, sir."))
}

fn default_folder(ctx: &Ctx, spoken: Option<&str>, fallback: &str) -> Result<PathBuf, String> {
    let name = spoken.unwrap_or(fallback);
    resolve::resolve_folder(name, ctx.policy, ctx.roots)
        .ok_or_else(|| format!("I don't know a folder called {name} that I'm allowed to use, sir."))
}

const EVERYTHING_REFUSAL: &str =
    "I won't delete everything or wipe a drive by voice, sir. Tell me exactly which files, like the PDFs in Downloads.";

pub fn handle(ctx: &Ctx, verb: &str, object: &str, folder: Option<&str>, dest: Option<&str>) -> Reply {
    let tool = match verb {
        "delete" => Tool::Delete,
        "move" => Tool::Move,
        "copy" => Tool::Copy,
        "rename" => Tool::Rename,
        "create_folder" => Tool::CreateFolder,
        "list" => Tool::List,
        "find" => Tool::Find,
        _ => return say("I didn't understand that file command, sir."),
    };
    if object == "everything" {
        crate::action_kernel::audit::record(
            ctx.dir,
            &crate::action_kernel::audit::AuditEntry::new(ctx.now, ctx.phrase, tool.name(), "forbidden", ctx.origin.as_str(), "deny", "blocked", "everything / wipe a drive"),
        );
        return say(EVERYTHING_REFUSAL);
    }
    match tool {
        Tool::CreateFolder => {
            let parent = match default_folder(ctx, folder, "documents") {
                Ok(p) => p,
                Err(e) => return say(e),
            };
            let req = Request { tool, sources: vec![], dest_dir: Some(parent), new_name: Some(object.to_string()) };
            return finish(ctx, req);
        }
        Tool::List | Tool::Find => {
            let f = match default_folder(ctx, folder, "downloads") {
                Ok(p) => p,
                Err(e) => return say(e),
            };
            let q = resolve::parse_object(object);
            return match files::list_card(ctx, &f, &q) {
                Ok((spoken, md)) => Reply::Card { spoken, title: "Files".into(), markdown: md },
                Err(r) => say(r.spoken),
            };
        }
        _ => {}
    }
    let folder_path = match default_folder(ctx, folder, "downloads") {
        Ok(p) => p,
        Err(e) => return say(e),
    };
    let q = resolve::parse_object(object);
    let found = match resolve::resolve(&folder_path, &q) {
        Resolved::Everything => return say(EVERYTHING_REFUSAL),
        Resolved::None => {
            return say(format!("I couldn't find anything matching that in {}, sir.", folder_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()))
        }
        Resolved::Ambiguous(c) => {
            return say(format!("I found several, sir: {}. Which one do you mean?", names_of(&c)));
        }
        Resolved::Found(c) => c,
    };
    let mut req = Request { tool, sources: found.iter().map(|c| c.path.clone()).collect(), dest_dir: None, new_name: None };
    match tool {
        Tool::Move | Tool::Copy => match resolve_dest(dest.unwrap_or(""), ctx) {
            Ok(d) => req.dest_dir = Some(d),
            Err(e) => return say(e),
        },
        Tool::Rename => {
            if found.len() != 1 {
                return say("I can only rename one thing at a time, sir.");
            }
            let mut name = dest.unwrap_or("").trim().to_string();
            if !name.contains('.') && !found[0].is_dir {
                if let Some((_, ext)) = found[0].name.rsplit_once('.') {
                    name = format!("{name}.{ext}");
                }
            }
            req.new_name = Some(name);
        }
        _ => {}
    }
    finish(ctx, req)
}

fn finish(ctx: &Ctx, req: Request) -> Reply {
    let plan = match files::plan(ctx, &req) {
        Ok(p) => p,
        Err(r) => return say(r.spoken),
    };
    match plan.verdict.decision.clone() {
        Decision::Deny(why) => say(why),
        Decision::Allow => say(files::execute(ctx, &plan, false).spoken),
        Decision::Ask(_) => {
            let (spoken, markdown) = files::confirmation(&plan);
            Reply::Confirm { spoken, markdown, plan: Box::new(plan) }
        }
    }
}

/// The user said yes to the held read-back.
pub fn confirm(ctx: &Ctx) -> String {
    match super::pending::take() {
        None => "I don't have anything waiting for your yes, sir.".to_string(),
        Some(plan) => {
            // the answer must come from a spoken turn, and the plan must have been made from one
            if plan.origin != super::Origin::UserVoice || ctx.origin != super::Origin::UserVoice {
                return "That didn't come from your voice, so I won't do it, sir.".into();
            }
            files::execute(ctx, &plan, true).spoken
        }
    }
}

pub fn decline(ctx: &Ctx) -> String {
    if let Some(p) = super::pending::take() {
        crate::action_kernel::audit::record(
            ctx.dir,
            &crate::action_kernel::audit::AuditEntry::new(ctx.now, &p.phrase, p.tool.name(), p.verdict.risk.as_str(), ctx.origin.as_str(), "declined", "blocked", ""),
        );
    }
    "Understood, sir. I left everything as it was.".into()
}

/// "Undo that".
pub fn undo(ctx: &Ctx) -> String {
    files::undo_last(ctx).spoken
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_kernel::files::{self, Request};
    use crate::action_kernel::journal::TRASH_DIR;
    use crate::action_kernel::paths::test_dir;
    use crate::action_kernel::policy::{test_env, Policy, PolicyEnv};
    use crate::action_kernel::Origin;
    use crate::action_kernel::Outcome;
    use crate::action_kernel::Tool;

    fn fa(t: &str) -> Option<(String, String, Option<String>, Option<String>)> {
        match parse(t) {
            Some(ParsedIntent::FileAction { verb, object, folder, dest }) => Some((verb, object, folder, dest)),
            _ => None,
        }
    }

    #[test]
    fn delete_phrases_parse_with_object_and_folder() {
        assert_eq!(fa("delete the pdfs in downloads"), Some(("delete".into(), "pdfs".into(), Some("downloads".into()), None)));
        assert_eq!(fa("Nexus, please delete my latest screenshot."), Some(("delete".into(), "latest screenshot".into(), None, None)));
        assert_eq!(fa("remove the old stuff folder from my desktop").unwrap().2.as_deref(), Some("desktop"));
        assert_eq!(fa("trash the installers").unwrap().1, "installers");
        assert_eq!(fa("get rid of tax report.pdf").unwrap().1, "tax report.pdf");
    }

    #[test]
    fn move_copy_rename_create_list_find_undo() {
        assert_eq!(
            fa("move the pdfs from downloads to documents"),
            Some(("move".into(), "pdfs".into(), Some("downloads".into()), Some("documents".into())))
        );
        assert_eq!(fa("copy my latest screenshot to the taxes folder in documents").unwrap().3.as_deref(), Some("taxes folder in documents"));
        assert_eq!(fa("rename tax report to taxes 2025").unwrap(), ("rename".into(), "tax report".into(), None, Some("taxes 2025".into())));
        assert_eq!(fa("make a new folder called invoices in documents").unwrap(), ("create_folder".into(), "invoices".into(), Some("documents".into()), None));
        assert_eq!(fa("what's in my downloads").unwrap(), ("list".into(), String::new(), Some("downloads".into()), None));
        assert_eq!(fa("list the pdfs in documents").unwrap().1, "pdfs");
        assert_eq!(fa("find tax report in downloads").unwrap().0, "find");
        for u in ["undo that", "undo it", "undo the last delete", "bring it back", "restore them", "put that back"] {
            assert!(matches!(parse(u), Some(ParsedIntent::FileUndo)), "{u}");
        }
    }

    #[test]
    fn broad_wipe_phrases_are_claimed_so_they_are_refused_locally() {
        for t in [
            "delete everything",
            "delete all my files",
            "wipe my c drive",
            "format my laptop",
            "delete all the files",
            "erase everything on my computer",
            "wipe my documents",
            "remove all my data",
            "delete my files",
            "destroy my hard drive",
        ] {
            let r = fa(t).unwrap_or_else(|| panic!("{t} must be claimed"));
            assert_eq!((r.0.as_str(), r.1.as_str()), ("delete", "everything"), "{t}");
        }
    }

    #[test]
    fn phrases_that_are_not_file_commands_are_left_alone() {
        for t in [
            "delete that message",
            "remove the background",
            "delete my chat with mummy",
            "clear my history",
            "remove the last word",
            "delete this tab",
            "delete the memory about my phone",
            "remove my account",
            "rename my laptop to nexus",
            "move the window to the left",
            "put the phone on silent",
            "search for pizza",
            "find my phone",
            "delete the reminder",
            "open downloads",
            "what's the weather",
            "empty the queue",
            "where is the nearest pharmacy",
        ] {
            assert!(parse(t).is_none(), "{t:?} must not be claimed: {:?}", parse(t));
        }
    }

    fn world() -> (PathBuf, PolicyEnv, Policy, Vec<PathBuf>) {
        let root = test_dir("voice");
        let (env, policy, roots) = test_env(&root);
        (root, env, policy, roots)
    }

    fn put(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"data").unwrap();
        p
    }

    #[test]
    fn full_conversation_delete_confirm_undo() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        crate::action_kernel::pending::clear();
        let (_root, env, mut policy, roots) = world();
        // the documents root is addressable by its name through an extra alias
        policy.extra_roots = vec![roots[0].to_string_lossy().to_string()];
        let data = env.data_dir.clone();
        let docs = roots[0].clone();
        put(&docs, "tax report.pdf");
        put(&docs, "resume.pdf");
        let mk = |origin| Ctx { dir: &data, policy: &policy, env: &env, roots: &roots, now: 1, phrase: "delete the pdfs", origin };
        let c = mk(Origin::UserVoice);
        let r = handle(&c, "delete", "pdfs", Some("documents"), None);
        let Reply::Confirm { spoken, plan, .. } = r else { panic!("must ask first") };
        assert!(spoken.contains("2 items") && spoken.ends_with("yes to confirm."), "{spoken}");
        assert!(docs.join("tax report.pdf").exists());
        crate::action_kernel::pending::hold(*plan);
        let said = confirm(&c);
        assert!(said.starts_with("Done, sir"), "{said}");
        assert!(!docs.join("tax report.pdf").exists() && !docs.join("resume.pdf").exists());
        assert!(undo(&c).starts_with("Done, sir"));
        assert!(docs.join("tax report.pdf").exists() && docs.join("resume.pdf").exists());
        // a second yes finds nothing
        assert!(confirm(&c).contains("anything waiting"));
    }

    #[test]
    fn ambiguous_everything_and_missing_never_act() {
        let (_root, env, mut policy, roots) = world();
        policy.extra_roots = vec![roots[0].to_string_lossy().to_string()];
        let data = env.data_dir.clone();
        let docs = roots[0].clone();
        put(&docs, "a.pdf");
        put(&docs, "b.pdf");
        let c = Ctx { dir: &data, policy: &policy, env: &env, roots: &roots, now: 1, phrase: "p", origin: Origin::UserVoice };
        let docs_name = docs.file_name().unwrap().to_string_lossy().to_string();
        let r = handle(&c, "delete", "pdf", Some(&docs_name), None);
        assert!(matches!(r, Reply::Say(ref s) if s.contains("several")));
        assert!(matches!(handle(&c, "delete", "everything", None, None), Reply::Say(ref s) if s.contains("won't delete everything")));
        assert!(matches!(handle(&c, "delete", "budget", Some(&docs_name), None), Reply::Say(ref s) if s.contains("couldn't find")));
        assert!(matches!(handle(&c, "delete", "all files", Some(&docs_name), None), Reply::Say(ref s) if s.contains("won't delete everything")));
        assert!(docs.join("a.pdf").exists() && docs.join("b.pdf").exists());
    }

    #[test]
    fn the_confirmation_must_come_from_a_user_voice_turn() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let (_root, env, mut policy, roots) = world();
        policy.extra_roots = vec![roots[0].to_string_lossy().to_string()];
        let data = env.data_dir.clone();
        let f = put(&roots[0], "keep.txt");
        let user = Ctx { dir: &data, policy: &policy, env: &env, roots: &roots, now: 1, phrase: "p", origin: Origin::UserVoice };
        let Reply::Confirm { plan, .. } = handle(&user, "delete", "keep.txt", Some(&roots[0].file_name().unwrap().to_string_lossy()), None) else {
            panic!("ask")
        };
        crate::action_kernel::pending::hold(*plan);
        let derived = Ctx { origin: Origin::Derived, ..Ctx { dir: &data, policy: &policy, env: &env, roots: &roots, now: 1, phrase: "p", origin: Origin::UserVoice } };
        assert!(confirm(&derived).contains("didn't come from your voice"));
        assert!(f.exists());
        // and a plan made by derived text is denied before it can even be held
        let r = handle(&derived, "delete", "keep.txt", Some(&roots[0].file_name().unwrap().to_string_lossy()), None);
        assert!(matches!(r, Reply::Say(ref s) if s.contains("didn't come from your voice")), "derived delete is refused outright");
        assert!(f.exists());
    }

    #[test]
    fn move_into_a_named_subfolder_and_unknown_destinations() {
        let (_root, env, mut policy, roots) = world();
        policy.extra_roots = vec![roots[0].to_string_lossy().to_string()];
        let data = env.data_dir.clone();
        let docs = roots[0].clone();
        let docs_name = docs.file_name().unwrap().to_string_lossy().to_string();
        std::fs::create_dir_all(docs.join("taxes")).unwrap();
        put(&docs, "invoice.pdf");
        let c = Ctx { dir: &data, policy: &policy, env: &env, roots: &roots, now: 1, phrase: "p", origin: Origin::UserVoice };
        let r = handle(&c, "move", "invoice", Some(&docs_name), Some(&format!("taxes folder in {docs_name}")));
        assert!(matches!(r, Reply::Say(ref s) if s.starts_with("Moved")), "{:?}", match r { Reply::Say(s) => s, _ => "other".into() });
        assert!(docs.join("taxes").join("invoice.pdf").exists());
        let r = handle(&c, "move", "invoice", Some(&format!("taxes folder in {docs_name}")), Some("nowhere"));
        assert!(matches!(r, Reply::Say(ref s) if s.contains("don't know")), "unknown dest or source folder is refused");
    }

    struct TempCleaner(PathBuf);
    impl Drop for TempCleaner {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_world(tag: &str) -> (PathBuf, PolicyEnv, Policy, Vec<PathBuf>, TempCleaner) {
        let root = test_dir(tag);
        let cleaner = TempCleaner(root.clone());
        let (env, mut policy, roots) = test_env(&root);
        policy.extra_roots = vec![roots[0].to_string_lossy().to_string()];
        (root, env, policy, roots, cleaner)
    }

    #[test]
    fn spoken_delete_confirm_undo_round_trip_on_real_files() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        crate::action_kernel::pending::clear();
        let (_root, env, policy, roots, _cleaner) = test_world("del_rt");
        let data = env.data_dir.clone();
        let folder = roots[0].clone();

        let a_path = folder.join("a.pdf");
        let b_path = folder.join("b.pdf");
        let notes_path = folder.join("notes.txt");
        let sub_dir = folder.join("sub");
        std::fs::create_dir_all(&sub_dir).unwrap();
        let c_path = sub_dir.join("c.pdf");

        let a_bytes = b"bytes of a.pdf";
        let b_bytes = b"bytes of b.pdf";
        let notes_bytes = b"bytes of notes.txt";
        let c_bytes = b"bytes of sub/c.pdf";

        std::fs::write(&a_path, a_bytes).unwrap();
        std::fs::write(&b_path, b_bytes).unwrap();
        std::fs::write(&notes_path, notes_bytes).unwrap();
        std::fs::write(&c_path, c_bytes).unwrap();

        let folder_alias = folder.file_name().unwrap().to_string_lossy().to_lowercase();
        let transcript = format!("delete the pdfs in {folder_alias}");

        // orchestrator parses transcript
        let parsed = parse(&transcript);
        let Some(ParsedIntent::FileAction { verb, object, folder: parsed_folder, dest }) = parsed else {
            panic!("voice::parse failed to parse {transcript}");
        };
        assert_eq!(verb, "delete");
        assert_eq!(object, "pdfs");

        let ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: &transcript,
            origin: Origin::UserVoice,
        };

        // orchestrator handles the file action
        let reply = handle(&ctx, &verb, &object, parsed_folder.as_deref(), dest.as_deref());
        let Reply::Confirm { spoken: confirm_prompt, plan, .. } = reply else {
            panic!("handle must return Reply::Confirm for delete");
        };
        assert!(confirm_prompt.contains("2 items"), "read-back prompt must mention count: {confirm_prompt}");

        // before confirm nothing moved
        assert!(a_path.exists());
        assert!(b_path.exists());
        assert!(notes_path.exists());
        assert!(c_path.exists());

        // orchestrator holds the plan and runs confirm on user yes
        crate::action_kernel::pending::hold(*plan);
        let confirm_reply = confirm(&ctx);
        assert!(confirm_reply.contains("2 items"), "confirm reply must mention count: {confirm_reply}");

        // after confirm pdf files are gone from their places
        assert!(!a_path.exists(), "a.pdf must be moved away");
        assert!(!b_path.exists(), "b.pdf must be moved away");
        // notes.txt untouched
        assert!(notes_path.exists());
        assert_eq!(std::fs::read(&notes_path).unwrap(), notes_bytes);
        // sub/c.pdf untouched
        assert!(c_path.exists());
        assert_eq!(std::fs::read(&c_path).unwrap(), c_bytes);

        // pdf files exist under .nexus-trash
        let j = crate::action_kernel::journal::load(&data);
        assert_eq!(j.quarantine.len(), 2);
        for q in &j.quarantine {
            assert!(q.stored.contains(TRASH_DIR));
            assert!(Path::new(&q.stored).exists(), "quarantined file must exist at {}", q.stored);
        }

        // orchestrator runs undo on user request
        let undo_reply = undo(&ctx);
        assert!(undo_reply.contains("2 items"), "undo reply must mention count: {undo_reply}");

        // after undo every file is back with identical bytes
        assert!(a_path.exists());
        assert_eq!(std::fs::read(&a_path).unwrap(), a_bytes);
        assert!(b_path.exists());
        assert_eq!(std::fs::read(&b_path).unwrap(), b_bytes);
        assert!(notes_path.exists());
        assert_eq!(std::fs::read(&notes_path).unwrap(), notes_bytes);
        assert!(c_path.exists());
        assert_eq!(std::fs::read(&c_path).unwrap(), c_bytes);

        // trash is empty of them
        for q in &j.quarantine {
            assert!(!Path::new(&q.stored).exists(), "trash must be empty of {}", q.stored);
        }
        let j_after = crate::action_kernel::journal::load(&data);
        assert!(j_after.waiting().is_empty());
    }

    #[test]
    fn decline_and_timeout_leave_everything_untouched() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        crate::action_kernel::pending::clear();
        let (_root, env, policy, roots, _cleaner) = test_world("decline_timeout");
        let data = env.data_dir.clone();
        let folder = roots[0].clone();

        let file_path = folder.join("precious.pdf");
        let content = b"do not delete me";
        std::fs::write(&file_path, content).unwrap();

        let ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "delete precious.pdf in documents",
            origin: Origin::UserVoice,
        };

        // hold a delete, call decline
        let reply = handle(&ctx, "delete", "precious.pdf", Some("documents"), None);
        let Reply::Confirm { plan, .. } = reply else { panic!("handle must return Reply::Confirm") };
        crate::action_kernel::pending::hold(*plan);

        let decline_msg = decline(&ctx);
        assert!(decline_msg.contains("left everything as it was"), "decline message: {decline_msg}");
        assert!(file_path.exists(), "file must stay after decline");
        assert_eq!(std::fs::read(&file_path).unwrap(), content);
        assert!(crate::action_kernel::pending::take().is_none(), "pending must be cleared after decline");

        // hold a delete, clear pending (simulating timeout), then confirm
        let reply2 = handle(&ctx, "delete", "precious.pdf", Some("documents"), None);
        let Reply::Confirm { plan: plan2, .. } = reply2 else { panic!("handle must return Reply::Confirm") };
        crate::action_kernel::pending::hold(*plan2);

        crate::action_kernel::pending::clear();

        let confirm_msg = confirm(&ctx);
        assert!(confirm_msg.contains("anything waiting"), "confirm on timed-out plan says nothing waiting: {confirm_msg}");
        assert!(file_path.exists(), "file must stay untouched after timeout");
        assert_eq!(std::fs::read(&file_path).unwrap(), content);
    }

    #[test]
    fn a_typed_or_non_voice_origin_cannot_confirm_a_delete() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        crate::action_kernel::pending::clear();
        let (_root, env, policy, roots, _cleaner) = test_world("non_voice");
        let data = env.data_dir.clone();
        let folder = roots[0].clone();

        let file_path = folder.join("guarded.pdf");
        let content = b"guarded file content";
        std::fs::write(&file_path, content).unwrap();

        let user_ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "delete guarded.pdf in documents",
            origin: Origin::UserVoice,
        };

        let reply = handle(&user_ctx, "delete", "guarded.pdf", Some("documents"), None);
        let Reply::Confirm { plan, .. } = reply else { panic!("user voice delete must ask for confirmation") };
        crate::action_kernel::pending::hold(*plan);

        let non_voice_ctx = Ctx {
            origin: Origin::Derived,
            phrase: "yes",
            ..user_ctx
        };

        let confirm_msg = confirm(&non_voice_ctx);
        assert!(
            confirm_msg.contains("didn't come from your voice"),
            "confirm must refuse non-voice origin: {confirm_msg}"
        );
        assert!(file_path.exists(), "file must remain untouched when non-voice confirms");
        assert_eq!(std::fs::read(&file_path).unwrap(), content);
    }

    #[test]
    fn move_copy_rename_never_overwrite() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        crate::action_kernel::pending::clear();
        let (_root, env, policy, roots, _cleaner) = test_world("no_overwrite");
        let data = env.data_dir.clone();
        let docs = roots[0].clone();
        let archive = docs.join("archive");
        std::fs::create_dir_all(&archive).unwrap();

        let ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "test",
            origin: Origin::UserVoice,
        };

        let docs_alias = docs.file_name().unwrap().to_string_lossy().to_string();

        // 1. Copy: picks safe name "name (2).ext" without overwriting destination
        let copy_src = docs.join("doc.txt");
        let copy_dest = archive.join("doc.txt");
        std::fs::write(&copy_src, b"source doc bytes").unwrap();
        std::fs::write(&copy_dest, b"existing archive bytes").unwrap();

        let copy_reply = handle(&ctx, "copy", "doc.txt", Some(&docs_alias), Some(&format!("archive folder in {docs_alias}")));
        let Reply::Say(copy_spoken) = copy_reply else { panic!("copy must return Reply::Say") };
        assert!(copy_spoken.starts_with("Copied"), "copy result: {copy_spoken}");
        assert_eq!(std::fs::read(&copy_dest).unwrap(), b"existing archive bytes");
        let safe_copy = archive.join("doc (2).txt");
        assert!(safe_copy.exists(), "safe copy name must exist");
        assert_eq!(std::fs::read(&safe_copy).unwrap(), b"source doc bytes");
        assert!(copy_src.exists(), "source must still exist after copy");

        // 2. Move: picks safe name "name (3).ext" without overwriting destination
        let move_reply = handle(&ctx, "move", "doc.txt", Some(&docs_alias), Some(&format!("archive folder in {docs_alias}")));
        let Reply::Say(move_spoken) = move_reply else { panic!("move must return Reply::Say") };
        assert!(move_spoken.starts_with("Moved"), "move result: {move_spoken}");
        assert_eq!(std::fs::read(&copy_dest).unwrap(), b"existing archive bytes");
        let safe_move = archive.join("doc (3).txt");
        assert!(safe_move.exists(), "safe move name must exist");
        assert_eq!(std::fs::read(&safe_move).unwrap(), b"source doc bytes");
        assert!(!copy_src.exists(), "source must be moved");

        // 3. Rename: refuses outright when destination name already exists
        let rename_src = docs.join("alpha.txt");
        let rename_target = docs.join("beta.txt");
        std::fs::write(&rename_src, b"alpha bytes").unwrap();
        std::fs::write(&rename_target, b"beta bytes").unwrap();

        let rename_reply = handle(&ctx, "rename", "alpha.txt", Some(&docs_alias), Some("beta.txt"));
        let Reply::Say(rename_spoken) = rename_reply else { panic!("rename must return Reply::Say") };
        assert!(
            rename_spoken.contains("already there") && rename_spoken.contains("won't overwrite"),
            "rename must refuse to overwrite: {rename_spoken}"
        );
        assert_eq!(std::fs::read(&rename_target).unwrap(), b"beta bytes");
        assert_eq!(std::fs::read(&rename_src).unwrap(), b"alpha bytes");
    }

    #[test]
    fn delete_everything_and_wipe_my_drive_are_refused_locally() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let (_root, env, policy, roots, _cleaner) = test_world("refuse_wipe");
        let data = env.data_dir.clone();

        let ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "delete everything",
            origin: Origin::UserVoice,
        };

        for phrase in [
            "delete everything",
            "wipe my drive",
            "wipe my c drive",
            "erase everything on my computer",
            "destroy my hard drive",
        ] {
            crate::action_kernel::pending::clear();
            let parsed = parse(phrase);
            let Some(ParsedIntent::FileAction { verb, object, folder, dest }) = parsed else {
                panic!("phrase {phrase:?} must be claimed by voice::parse");
            };
            assert_eq!(verb, "delete", "{phrase}");
            assert_eq!(object, "everything", "{phrase}");

            let reply = handle(&ctx, &verb, &object, folder.as_deref(), dest.as_deref());
            let Reply::Say(spoken) = reply else {
                panic!("{phrase}: broad wipe commands must return Reply::Say refusal, not confirmation");
            };
            assert!(
                spoken.contains("won't delete everything or wipe a drive"),
                "{phrase}: refusal message mismatch: {spoken}"
            );
            assert!(
                crate::action_kernel::pending::take().is_none(),
                "{phrase}: nothing must be held in pending"
            );
        }
    }

    #[test]
    fn forbidden_places_are_never_touched() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let (root, env, policy, roots, _cleaner) = test_world("forbidden");
        // The temp folders live under the real users folder, which the policy treats as "another user";
        // take that rule out of the picture so each case below is decided by the rule it is meant to test.
        let mut env = env;
        env.users_dir = None;
        let data = env.data_dir.clone();
        let docs = roots[0].clone();

        let ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "test forbidden",
            origin: Origin::UserVoice,
        };

        // 1. Outside the allowed roots
        let outside_dir = root.join("unallowed_folder");
        std::fs::create_dir_all(&outside_dir).unwrap();
        let outside_file = outside_dir.join("outside.txt");
        std::fs::write(&outside_file, b"outside content").unwrap();

        let req_outside = Request { tool: Tool::Delete, sources: vec![outside_file.clone()], dest_dir: None, new_name: None };
        let p_outside = files::plan(&ctx, &req_outside).unwrap();
        assert!(matches!(p_outside.verdict.decision, Decision::Deny(ref w) if w.contains("outside the folders")), "{:?}", p_outside.verdict.decision);
        let r_outside = files::execute(&ctx, &p_outside, true);
        assert_eq!(r_outside.outcome, Outcome::Blocked);
        assert_eq!(std::fs::read(&outside_file).unwrap(), b"outside content");

        // 2. Windows system folder
        let sys_file = env.system_dirs[0].join("kernel_probe.dll");
        std::fs::create_dir_all(&env.system_dirs[0]).unwrap();
        std::fs::write(&sys_file, b"system file bytes").unwrap();

        let req_sys = Request { tool: Tool::Delete, sources: vec![sys_file.clone()], dest_dir: None, new_name: None };
        let p_sys = files::plan(&ctx, &req_sys).unwrap();
        assert!(matches!(p_sys.verdict.decision, Decision::Deny(ref w) if w.contains("part of Windows")));
        let r_sys = files::execute(&ctx, &p_sys, true);
        assert_eq!(r_sys.outcome, Outcome::Blocked);
        assert_eq!(std::fs::read(&sys_file).unwrap(), b"system file bytes");

        // 3. The app's own data folder
        let data_file = env.data_dir.join("app_secret.json");
        std::fs::write(&data_file, b"data file bytes").unwrap();

        let req_data = Request { tool: Tool::Delete, sources: vec![data_file.clone()], dest_dir: None, new_name: None };
        let p_data = files::plan(&ctx, &req_data).unwrap();
        assert!(matches!(p_data.verdict.decision, Decision::Deny(ref w) if w.contains("my own data")));
        let r_data = files::execute(&ctx, &p_data, true);
        assert_eq!(r_data.outcome, Outcome::Blocked);
        assert_eq!(std::fs::read(&data_file).unwrap(), b"data file bytes");

        // 4. Protected .git folder inside an allowed root
        let git_dir = docs.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let git_file = git_dir.join("config");
        std::fs::write(&git_file, b"git config bytes").unwrap();

        let req_git = Request { tool: Tool::Delete, sources: vec![git_file.clone()], dest_dir: None, new_name: None };
        let p_git = files::plan(&ctx, &req_git).unwrap();
        assert!(matches!(p_git.verdict.decision, Decision::Deny(ref w) if w.contains("protected location")));
        let r_git = files::execute(&ctx, &p_git, true);
        assert_eq!(r_git.outcome, Outcome::Blocked);
        assert_eq!(std::fs::read(&git_file).unwrap(), b"git config bytes");
    }

    #[test]
    fn unicode_spaces_and_long_names_survive_the_trash_round_trip() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        crate::action_kernel::pending::clear();
        let (_root, env, policy, roots, _cleaner) = test_world("unicode_long");
        let data = env.data_dir.clone();
        let docs = roots[0].clone();

        let unicode_name = "my report \u{e9}.pdf";
        let long_name = format!("{}.pdf", "a".repeat(146));
        assert_eq!(long_name.len(), 150);

        let f1 = docs.join(unicode_name);
        let f2 = docs.join(&long_name);
        let bytes1 = b"unicode report \xc3\xa9 content";
        let bytes2 = b"150-char name content bytes";

        std::fs::write(&f1, bytes1).unwrap();
        std::fs::write(&f2, bytes2).unwrap();

        let ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "delete the pdfs in documents",
            origin: Origin::UserVoice,
        };

        let docs_alias = docs.file_name().unwrap().to_string_lossy().to_string();
        let reply = handle(&ctx, "delete", "pdfs", Some(&docs_alias), None);
        let Reply::Confirm { plan, spoken, .. } = reply else {
            panic!("handle must ask for confirmation on delete")
        };
        assert!(spoken.contains("2 items"), "read-back mentions count: {spoken}");

        crate::action_kernel::pending::hold(*plan);
        let confirm_spoken = confirm(&ctx);
        assert!(confirm_spoken.contains("2 items"), "confirm mentions count: {confirm_spoken}");

        assert!(!f1.exists());
        assert!(!f2.exists());

        let j = crate::action_kernel::journal::load(&data);
        assert_eq!(j.quarantine.len(), 2);
        for q in &j.quarantine {
            assert!(Path::new(&q.stored).exists());
        }

        let undo_spoken = undo(&ctx);
        assert!(undo_spoken.contains("2 items"), "undo mentions count: {undo_spoken}");

        assert!(f1.exists(), "unicode and spaces file must be restored");
        assert_eq!(std::fs::read(&f1).unwrap(), bytes1);

        assert!(f2.exists(), "150-character name file must be restored");
        assert_eq!(std::fs::read(&f2).unwrap(), bytes2);
    }

    #[test]
    fn undo_with_nothing_to_undo_and_double_undo_are_harmless() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        crate::action_kernel::pending::clear();
        let (_root, env, policy, roots, _cleaner) = test_world("double_undo");
        let data = env.data_dir.clone();
        let docs = roots[0].clone();

        let ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "test undo",
            origin: Origin::UserVoice,
        };

        // 1. Undo with nothing to undo
        let res0 = undo(&ctx);
        assert!(res0.contains("nothing to undo"), "undo when empty: {res0}");

        // 2. Perform one delete action
        let f = docs.join("temp.pdf");
        std::fs::write(&f, b"temp data").unwrap();

        let docs_alias = docs.file_name().unwrap().to_string_lossy().to_string();
        let reply = handle(&ctx, "delete", "temp.pdf", Some(&docs_alias), None);
        let Reply::Confirm { plan, .. } = reply else { panic!("delete asks") };
        crate::action_kernel::pending::hold(*plan);
        let _ = confirm(&ctx);
        assert!(!f.exists());

        // 3. First undo reverses the action
        let res1 = undo(&ctx);
        assert!(res1.starts_with("Done, sir. I undid"), "first undo: {res1}");
        assert!(f.exists());

        // 4. Second undo in a row says nothing to undo
        let res2 = undo(&ctx);
        assert!(res2.contains("nothing to undo"), "second undo: {res2}");
        assert!(f.exists());
    }

    #[test]
    fn a_name_clash_in_the_trash_does_not_lose_a_file() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        crate::action_kernel::pending::clear();
        let (_root, env, policy, roots, _cleaner) = test_world("trash_clash");
        let data = env.data_dir.clone();
        let docs = roots[0].clone();
        let docs_alias = docs.file_name().unwrap().to_string_lossy().to_string();

        let f = docs.join("a.pdf");
        let v1_bytes = b"version 1 contents";
        let v2_bytes = b"version 2 contents";

        // 1. Delete a.pdf (version 1)
        std::fs::write(&f, v1_bytes).unwrap();
        let ctx1 = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "delete a.pdf in documents",
            origin: Origin::UserVoice,
        };
        let Reply::Confirm { plan: p1, .. } = handle(&ctx1, "delete", "a.pdf", Some(&docs_alias), None) else { panic!() };
        crate::action_kernel::pending::hold(*p1);
        let c1 = confirm(&ctx1);
        assert!(c1.starts_with("Done, sir"));
        assert!(!f.exists());

        // 2. Create a NEW a.pdf at the same place
        std::fs::write(&f, v2_bytes).unwrap();

        // 3. Delete a.pdf again (version 2)
        let ctx2 = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 2000,
            phrase: "delete a.pdf in documents",
            origin: Origin::UserVoice,
        };
        let Reply::Confirm { plan: p2, .. } = handle(&ctx2, "delete", "a.pdf", Some(&docs_alias), None) else { panic!() };
        crate::action_kernel::pending::hold(*p2);
        let c2 = confirm(&ctx2);
        assert!(c2.starts_with("Done, sir"));
        assert!(!f.exists());

        // Check journal: both versions are recorded in quarantine
        let j = crate::action_kernel::journal::load(&data);
        assert_eq!(j.quarantine.len(), 2);
        assert_eq!(j.batches.len(), 2);
        assert!(j.quarantine.iter().all(|q| q.original.ends_with("a.pdf")));

        // 4. Undo twice: both versions are recoverable
        let u1 = undo(&ctx2);
        assert!(u1.starts_with("Done, sir"));
        assert!(f.exists());
        assert_eq!(std::fs::read(&f).unwrap(), v2_bytes, "version 2 restored first to a.pdf");

        let u2 = undo(&ctx2);
        assert!(u2.starts_with("Done, sir"));

        // Version 2 remains at a.pdf, and version 1 was restored without overwrite as a (2).pdf
        assert_eq!(std::fs::read(&f).unwrap(), v2_bytes);
        let f_clash = docs.join("a (2).pdf");
        assert!(f_clash.exists(), "clashing restored file must be named a (2).pdf");
        assert_eq!(std::fs::read(&f_clash).unwrap(), v1_bytes);

        // Journal shows both restored
        let j_final = crate::action_kernel::journal::load(&data);
        assert!(j_final.quarantine.iter().all(|q| q.restored));
        assert!(j_final.batches.iter().all(|b| b.undone));
    }

    #[test]
    fn list_and_find_return_results_without_changing_anything() {
        let _g = crate::action_kernel::pending::TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let (_root, env, policy, roots, _cleaner) = test_world("list_find");
        let data = env.data_dir.clone();
        let docs = roots[0].clone();

        let f1 = docs.join("annual_report.pdf");
        let f2 = docs.join("budget_summary.xlsx");
        let bytes1 = b"annual report data 2026";
        let bytes2 = b"budget spreadsheet numbers";

        std::fs::write(&f1, bytes1).unwrap();
        std::fs::write(&f2, bytes2).unwrap();

        let ctx = Ctx {
            dir: &data,
            policy: &policy,
            env: &env,
            roots: &roots,
            now: 1000,
            phrase: "test",
            origin: Origin::UserVoice,
        };

        // 1. List
        let list_transcript = "what's in documents";
        let Some(ParsedIntent::FileAction { verb: list_verb, object: list_obj, folder: list_f, dest: list_dest }) = parse(list_transcript) else {
            panic!("parse failed for {list_transcript}");
        };
        assert_eq!(list_verb, "list");

        let list_reply = handle(&ctx, &list_verb, &list_obj, list_f.as_deref(), list_dest.as_deref());
        let Reply::Card { spoken: list_spoken, markdown: list_md, .. } = list_reply else {
            panic!("list must return Reply::Card");
        };
        assert!(list_spoken.contains("2 items"), "list spoken counts files: {list_spoken}");
        assert!(list_md.contains("annual_report.pdf"), "list markdown mentions annual_report: {list_md}");
        assert!(list_md.contains("budget_summary.xlsx"), "list markdown mentions budget_summary: {list_md}");

        // 2. Find
        let find_transcript = "find report in documents";
        let Some(ParsedIntent::FileAction { verb: find_verb, object: find_obj, folder: find_f, dest: find_dest }) = parse(find_transcript) else {
            panic!("parse failed for {find_transcript}");
        };
        assert_eq!(find_verb, "find");

        let find_reply = handle(&ctx, &find_verb, &find_obj, find_f.as_deref(), find_dest.as_deref());
        let Reply::Card { spoken: find_spoken, markdown: find_md, .. } = find_reply else {
            panic!("find must return Reply::Card");
        };
        assert!(find_spoken.contains("1 item"), "find spoken counts matching item: {find_spoken}");
        assert!(find_md.contains("annual_report.pdf"), "find markdown mentions annual_report: {find_md}");
        assert!(!find_md.contains("budget_summary.xlsx"), "find markdown does not mention non-matching: {find_md}");

        // 3. Folder is byte-identical afterwards
        assert!(f1.exists());
        assert_eq!(std::fs::read(&f1).unwrap(), bytes1);
        assert!(f2.exists());
        assert_eq!(std::fs::read(&f2).unwrap(), bytes2);

        let count = std::fs::read_dir(&docs).unwrap().filter_map(|e| e.ok()).count();
        assert_eq!(count, 2, "no new files created or deleted");
    }
}
