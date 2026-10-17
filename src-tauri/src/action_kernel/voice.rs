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
    use crate::action_kernel::paths::test_dir;
    use crate::action_kernel::policy::{test_env, Policy, PolicyEnv};
    use crate::action_kernel::Origin;

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
}
