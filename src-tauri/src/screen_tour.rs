//! Narrated screen tour — "Nexus, analyse my screen".
//!
//! The overlay points at one thing at a time (a ring on the target + a
//! callout box with a leader line) while TTS explains exactly that thing;
//! the long-form analysis goes to the sidebar. This module is the PURE core
//! (script parsing/validation, word budgets, narration plan, the
//! one-callout-at-a-time state machine) plus the small Gemini glue. The
//! runner that wires it to capture / TTS / events lives in
//! `orchestrator::run_screen_tour`.
//!
//! Sync guarantee (the user's hard requirement): the frontend only ever
//! receives the CURRENT callout. `TourEngine` emits `Show(i+1)` strictly
//! after `Clear` for `i`, driven by `tts::narrate` queue events (line i is
//! audible ⇔ callout i is shown) — nothing is drawn ahead of speech.

use crate::vision::{
    denormalize_spatial_box, normalize_box, strip_json_fences, SpatialAnalysisItem,
    SpatialAnalysisPayload, SpatialBoundingBox, SpatialDetailRow,
};
use tauri::{AppHandle, Manager, Runtime};

pub const MAX_ITEMS: usize = 5;
/// The spoken overview IS the direct answer ("seven kinds: …"), so it needs
/// room — but it stays an overview; the long form goes to the sidebar.
pub const OVERVIEW_MAX_WORDS: usize = 35;
pub const SPOKEN_MAX_WORDS: usize = 22;
pub const CALLOUT_TITLE_MAX_WORDS: usize = 4;
pub const CALLOUT_TEXT_MAX_WORDS: usize = 24;
/// Sidebar header answer (read, not spoken).
pub const ANSWER_MAX_WORDS: usize = 120;
/// Total spoken words (overview + items + closer) — keeps the tour a short
/// overview no matter how long the model's analysis is (~45 s of speech max).
pub const MAX_TOTAL_SPOKEN_WORDS: usize = 120;
pub const CLOSER: &str = "Full breakdown is in the sidebar, sir.";
const FALLBACK_OVERVIEW: &str = "Here is what I see on your screen, sir.";

/// Short acknowledgements spoken right after the command is understood.
pub const SCREEN_ACKS: &[&str] = &[
    "On it, sir.",
    "Ok, sir.",
    "Sure, sir.",
    "Right away, sir.",
    "Analyzing your screen now, sir.",
];

/// Ack text for a seed byte (random in production, fixed in tests). Pure.
pub fn screen_ack(seed: u8) -> &'static str {
    SCREEN_ACKS[seed as usize % SCREEN_ACKS.len()]
}

// ─── Drawing style & intent ────────────────────────────────────────────

/// How the overlay should draw the pointer for one item (optional `draw`
/// callout field). The frontend owns the actual drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawStyle {
    Ring,
    Underline,
    Bracket,
    Circle,
    Arrow,
    Highlight,
    Spotlight,
    Scan,
    Lens,
    Connect,
    Pin,
}

impl DrawStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            DrawStyle::Ring => "ring",
            DrawStyle::Underline => "underline",
            DrawStyle::Bracket => "bracket",
            DrawStyle::Circle => "circle",
            DrawStyle::Arrow => "arrow",
            DrawStyle::Highlight => "highlight",
            DrawStyle::Spotlight => "spotlight",
            DrawStyle::Scan => "scan",
            DrawStyle::Lens => "lens",
            DrawStyle::Connect => "connect",
            DrawStyle::Pin => "pin",
        }
    }

    /// Tolerant parse: trimmed, case-insensitive. Pure.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ring" => Some(DrawStyle::Ring),
            "underline" => Some(DrawStyle::Underline),
            "bracket" => Some(DrawStyle::Bracket),
            "circle" => Some(DrawStyle::Circle),
            "arrow" => Some(DrawStyle::Arrow),
            "highlight" => Some(DrawStyle::Highlight),
            "spotlight" => Some(DrawStyle::Spotlight),
            "scan" => Some(DrawStyle::Scan),
            "lens" => Some(DrawStyle::Lens),
            "connect" => Some(DrawStyle::Connect),
            "pin" => Some(DrawStyle::Pin),
            _ => None,
        }
    }

    /// Sensible pointer for a model item category. Pure.
    pub fn default_for_category(category: &str) -> Self {
        match category.trim().to_ascii_lowercase().as_str() {
            "person" | "product" | "food" | "object" | "other" | "" => DrawStyle::Ring,
            "ui_element" => DrawStyle::Arrow,
            "text_block" | "code" => DrawStyle::Highlight,
            "chart" | "diagram" => DrawStyle::Bracket,
            _ => DrawStyle::Ring,
        }
    }
}

impl Default for DrawStyle {
    fn default() -> Self {
        DrawStyle::Ring
    }
}

/// What the user really wants from the screen answer. Shapes the narration
/// and the order/relationship of the items. Pure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Explain,
    Find,
    Compare,
    Summarize,
    Decide,
    Debug,
}

impl Intent {
    pub fn as_str(self) -> &'static str {
        match self {
            Intent::Explain => "explain",
            Intent::Find => "find",
            Intent::Compare => "compare",
            Intent::Summarize => "summarize",
            Intent::Decide => "decide",
            Intent::Debug => "debug",
        }
    }

    /// Tolerant parse (trimmed, case-insensitive; accepts the near variants
    /// a model may emit). Pure.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "explain" | "explaining" | "explains" | "explanation" | "describe" | "description"
            | "analyse" | "analyze" | "analysis" => Some(Intent::Explain),
            "find" | "finding" | "locate" | "locating" | "search" => Some(Intent::Find),
            "compare" | "comparing" | "comparison" => Some(Intent::Compare),
            "summarize" | "summarise" | "summarizing" | "summarising" | "summary" => {
                Some(Intent::Summarize)
            }
            "decide" | "deciding" | "decision" | "choose" | "recommend" | "recommendation" => {
                Some(Intent::Decide)
            }
            "debug" | "debugging" | "debugged" | "diagnose" | "diagnosing" | "diagnosis"
            | "error" | "troubleshoot" | "troubleshooting" => Some(Intent::Debug),
            _ => None,
        }
    }
}

impl Default for Intent {
    fn default() -> Self {
        Intent::Explain
    }
}

// ─── Script types ──────────────────────────────────────────────────────

/// One thing to point at and explain.
#[derive(Debug, Clone, PartialEq)]
pub struct TourItem {
    /// Sidebar card (id renumbered 1..n — card N == callout N).
    pub spatial: SpatialAnalysisItem,
    pub spoken: String,
    pub callout_title: String,
    pub callout_text: String,
    /// False when the box covers most of the screen: no ring/pointer, the
    /// callout appears in its default zone.
    pub has_target: bool,
    /// Overlay pointer style for this item.
    pub draw: DrawStyle,
    /// 0-based index (in `TourScript::items`, after renumbering) of the item
    /// this one relates to. Only meaningful for `DrawStyle::Connect`, and
    /// never its own index.
    pub link_to: Option<usize>,
    /// Small crop data URL, sent with the callout only for `DrawStyle::Lens`.
    pub thumb: Option<String>,
    /// Detected technology ids this item plausibly uses (reverse-engineer
    /// mode). Lowercased, de-duplicated, at most 4 — empty for a normal tour.
    pub stack_refs: Vec<String>,
    /// Main website domain of a recognisable product/app/service the item
    /// shows (e.g. `gmail.com`), already cross-checked against the item's own
    /// text; resolved to a logo by `brand_logo`. None when absent or doubtful.
    pub brand: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TourScript {
    pub title: String,
    /// What the user really wants (explain / find / compare / …).
    pub intent: Intent,
    /// Spoken first: the direct answer in ≤ OVERVIEW_MAX_WORDS words.
    pub overview: String,
    /// Longer written answer for the sidebar header (may be empty).
    pub answer: String,
    pub items: Vec<TourItem>,
    /// Screen rectangle (physical px: x, y, w, h) the model's 0–1000 boxes
    /// are relative to — the cropped page content, or the whole monitor.
    /// (0,0,0,0) = unset (callers must set it before drawing).
    pub region: (i32, i32, i32, i32),
}

impl TourScript {
    /// Sidebar payload (existing Spatial view, zero sidebar changes). The
    /// written `answer` (when the model gave one) is the sidebar overview.
    pub fn to_payload(&self) -> SpatialAnalysisPayload {
        SpatialAnalysisPayload {
            title: self.title.clone(),
            overview: if self.answer.is_empty() {
                self.overview.clone()
            } else {
                self.answer.clone()
            },
            items: self.items.iter().map(|i| i.spatial.clone()).collect(),
            provider_used: "gemini".to_string(),
            screen_image_data_url: None,
        }
    }
}

// ─── Prompt ────────────────────────────────────────────────────────────

/// Tour prompt: answer-first, content-aware, JSON contract + hard word
/// budgets. `context_block` is `ScreenContext::prompt_block()` (app, tab
/// title, trimmed URL, YouTube title/channel, whether the image is cropped).
/// Pure.
pub fn build_tour_prompt(user_prompt: &str, context_block: &str) -> String {
    format!(
        "You are NEXUS, a desktop assistant explaining the user's screen out loud while pointing at it.\n\
User request: \"{user}\"\n\n\
{ctx}\n\n\
Your job: work out what the MAIN CONTENT is, then explain it and answer the question the user \
implicitly asked.\n\
1. Decide what this is: an article or list, a video, a product page, code, a chat, a document, \
a chart, an application screen…\n\
2. Answer the implicit question. For a list or collection (\"kinds of nuts\", \"top 10 …\", a menu) \
the overview must say how many there are and NAME them. For a video, say what it is about using \
the title/channel above and what the frame shows. For a product, say what it is and the key facts \
read from the screen. For code or an error, say what it does or the likely cause.\n\
3. Items are the distinct SUBJECTS of the content (each bowl, product, chart, key paragraph, \
person as described by the page) — NEVER the browser, tabs, address bar, taskbar, ads or cookie \
banners unless one of those is the subject. Order by importance. At most {max_items} items; the \
overview already names the rest.\n\
4. People: describe each visible person and their on-screen action. Do not infer who someone is \
from their face. A visible caption or page label may be reported as text; associate a name with a \
person only if the screen explicitly makes that connection. If uncertain, use 'Person 1' and say \
what is visible. Never invent a biography or private facts.\n\
5. Describe only what you can see or what the context above states. If text is unreadable or \
something is unknown, say so instead of guessing.\n\
6. Narrate like a helpful guide: answer directly, then give each pointed item one concrete reason \
it matters to the user's question. Vary sentence openings instead of repeating 'This is' or \
'I can see'. Mention a visible relationship between items only when the image supports it. \
Do not add filler, directions to click, or a repeated honorific to every line.\n\
7. Choose HOW to point at each item (the \"draw\" field): \"ring\" is the default for objects, \
people and products; use \"highlight\" or \"underline\" for text; \"bracket\" for charts, diagrams \
or code blocks; \"arrow\" for a small control or button; \"circle\" for one small thing among \
many; \"spotlight\" when one region should dominate (use for at most one item); \"scan\" to sweep \
a table, list or code block; \"lens\" to magnify tiny text or a fine detail (the box must be \
small); \"connect\" to relate two items (then set \"link_to\" to the other item's id); \"pin\" \
for numbered steps.\n\
8. Decide the user's real INTENT — explain, find a thing, compare things, summarize, decide \
between options, or debug an error — and let it shape the answer: for \"find\" the first item must \
be the thing found; for \"compare\" use \"connect\" between the two compared items; for \"decide\" \
end with a clear recommendation in \"overview\"; for \"debug\" point at the error first and say \
the likely cause.\n\n\
Reply with ONLY a raw JSON object:\n\
{{\"title\": ..., \"intent\": \"explain|find|compare|summarize|decide|debug\", \"overview\": ..., \
\"answer\": ..., \"items\": [{{\"id\": 1, \"label\": ..., \
\"category\": ..., \"box_2d\": [ymin, xmin, ymax, xmax], \"confidence\": 0.0-1.0, \"spoken\": ..., \
\"callout_title\": ..., \"callout_text\": ..., \"draw\": \"<style>\", \"link_to\": <id or null>, \
\"brand\": <domain or null>, \"summary\": ..., \
\"details\": [{{\"title\": ..., \"value\": ...}}]}}]}}\n\n\
Field rules:\n\
- title: a short name for what this is (e.g. \"Healthiest nuts — Women's Health\").\n\
- intent: what the user really wants — explain, find, compare, summarize, decide or debug.\n\
- overview: spoken, at most {ov} words, the direct answer / what this is (plain words, no markdown).\n\
- answer: written for a sidebar, at most {ans} words — the fuller answer (lists welcome).\n\
- box_2d: integers 0-1000 normalised to the image (0,0 = top-left); frame the thing tightly.\n\
- category: one of [object, ui_element, text_block, chart, diagram, code, product, food, person, other].\n\
- draw: one of [ring, underline, bracket, circle, arrow, highlight, spotlight, scan, lens, connect, pin].\n\
- link_to: for \"connect\" only — the id of the other item; otherwise omit or null.\n\
- brand: only when the item is a recognisable product, app, service or company (its logo or icon is \
shown, e.g. \"gmail.com\", \"slack.com\", \"github.com\"): its main website domain, lowercase, no \
scheme or path, and the label must contain that brand's name. Otherwise null. Never guess.\n\
- label: the specific name (\"Walnuts\", \"Hazelnuts\"), not a type (\"item\").\n\
- spoken: ONE sentence, at most {sp} words, said while this item is pointed at: what it is plus one \
useful fact. Plain words, no markdown, no coordinates.\n\
- callout_title: at most {ct} words. callout_text: at most {cx} words — a different fact than spoken.\n\
- summary: two or three sentences. details: 3-6 rows {{title, value}} of specifics read from the \
screen or stated by the context (names, numbers, prices, claims).",
        user = user_prompt.trim(),
        ctx = context_block.trim(),
        max_items = MAX_ITEMS,
        ov = OVERVIEW_MAX_WORDS,
        ans = ANSWER_MAX_WORDS,
        sp = SPOKEN_MAX_WORDS,
        ct = CALLOUT_TITLE_MAX_WORDS,
        cx = CALLOUT_TEXT_MAX_WORDS,
    )
}

/// Reverse-engineer prompt: the base tour prompt (same JSON contract and
/// spoken word budgets) plus a mode block that asks HOW each section is
/// built and ties it to the detected stack. `techs` are `(id, display name)`
/// pairs from the scanner; `prior_sections` are the section names already
/// covered by earlier viewports. Pure.
pub fn build_reverse_prompt(
    user_prompt: &str,
    context_block: &str,
    stack_brief: &str,
    techs: &[(String, String)],
    viewport_idx: usize,
    viewport_total_hint: usize,
    prior_sections: &[String],
) -> String {
    let base = build_tour_prompt(user_prompt, context_block);
    let known = if techs.is_empty() {
        "none".to_string()
    } else {
        techs
            .iter()
            .map(|(id, name)| format!("{} ({})", id.trim(), name.trim()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let stack = {
        let s = stack_brief.trim();
        if s.is_empty() { "none detected".to_string() } else { s.to_string() }
    };
    let prior = {
        let joined = prior_sections
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
        if joined.is_empty() { "none".to_string() } else { joined }
    };
    format!(
        "{base}\n\n\
REVERSE ENGINEER MODE. The user wants to understand how this website is built. This image is \
section {idx} of about {total} of the page (scrolled top to bottom). Detected technology stack \
(from the page's code, not from the image): {stack}. Known technology ids you may reference: \
{known}.\n\
For each item explain HOW it is built and how one could recreate it: layout technique (grid, flex, \
sticky, full-bleed hero…), the component type (nav, carousel, pricing table, form…), notable \
visual effects/animations, and where the screen supports it, which detected technology likely \
produces it. Only claim a technology link when the screen or the stack list plausibly supports it; \
otherwise leave \"stack_refs\" empty. Never present a guess as fact: say 'likely' when inferring. \
Do not repeat sections already covered: {prior}. At most 4 items. If this looks like the page \
footer or the last section, say so in the overview. Start the overview by naming the section \
(for example \"The hero section…\").\n\
Add to every item an optional field \"stack_refs\": array of technology ids from the list above \
(at most 3).",
        idx = viewport_idx + 1,
        total = viewport_total_hint,
        stack = stack,
        known = known,
        prior = prior,
        base = base,
    )
}

// ─── Validation (pure) ─────────────────────────────────────────────────

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Cut `s` to at most `max` words — at a sentence end when one falls inside
/// the budget, else hard at `max` words. `period` adds a final '.' when the
/// cut text has no terminal punctuation (spoken lines). Pure.
pub fn limit_words(s: &str, max: usize, period: bool) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    let mut take = words.len().min(max);
    if words.len() > max {
        // Prefer the last sentence end inside the budget (if it keeps ≥ half).
        if let Some(pos) = words[..take]
            .iter()
            .rposition(|w| w.ends_with('.') || w.ends_with('!') || w.ends_with('?'))
        {
            if pos + 1 >= (max + 1) / 2 {
                take = pos + 1;
            }
        }
    }
    let mut out = words[..take].join(" ");
    if words.len() > max {
        out = out.trim_end_matches([',', ';', ':', '-', '—']).to_string();
    }
    if period && !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

fn box_area_fraction(b: &SpatialBoundingBox) -> f32 {
    (b.w_units() as f32 * b.h_units() as f32) / 1_000_000.0
}

fn iou(a: &SpatialBoundingBox, b: &SpatialBoundingBox) -> f32 {
    let ix = a.xmax.min(b.xmax).saturating_sub(a.xmin.max(b.xmin)) as f32;
    let iy = a.ymax.min(b.ymax).saturating_sub(a.ymin.max(b.ymin)) as f32;
    let inter = ix * iy;
    let union = a.w_units() as f32 * a.h_units() as f32
        + b.w_units() as f32 * b.h_units() as f32
        - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Recover complete `items` objects from truncated model JSON (a long
/// script can run out of output tokens mid-array). Returns (head object
/// with title/overview if recoverable, complete item objects). Pure.
fn salvage(text: &str) -> Option<(serde_json::Value, Vec<serde_json::Value>)> {
    let key = text.find("\"items\"")?;
    let arr_start = key + text[key..].find('[')?;
    let head_src = text[..key].trim_end().trim_end_matches(',');
    let head = head_src
        .find('{')
        .and_then(|s| serde_json::from_str(&format!("{}}}", &head_src[s..])).ok())
        .unwrap_or(serde_json::Value::Null);

    let body = &text[arr_start + 1..];
    let mut items = Vec::new();
    let (mut depth, mut start) = (0usize, None::<usize>);
    let (mut in_str, mut esc) = (false, false);
    for (i, c) in body.char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(s) = start.take() {
                        if let Ok(v) = serde_json::from_str(&body[s..=i]) {
                            items.push(v);
                        }
                    }
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    if items.is_empty() {
        None
    } else {
        Some((head, items))
    }
}

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// The brand's own name inside a domain: `gmail.com` -> `gmail`,
/// `mail.google.co.uk` -> `google`. None for shapes that are not a plain
/// hostname. Pure.
fn brand_name_of_domain(domain: &str) -> Option<String> {
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|l| l.is_empty()) {
        return None;
    }
    let mut end = labels.len() - 1; // drop the TLD
    if end >= 2 && matches!(labels[end - 1], "co" | "com" | "org" | "net" | "gov" | "ac") {
        end -= 1; // two-part suffix such as co.uk
    }
    let name = labels[end - 1];
    if name.len() < 3 {
        return None;
    }
    Some(name.to_string())
}

/// Optional per-item `brand` (a website domain). Kept only when it is a plain
/// lowercase hostname AND the brand's name appears in the item's own text, so
/// a model guess can never put the wrong company's logo on a card. Pure.
fn parse_brand(raw: &serde_json::Value, label: &str, callout_title: &str, summary: &str) -> Option<String> {
    let d = raw.get("brand")?.as_str()?.trim().to_ascii_lowercase();
    let d = d
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_string();
    if d.is_empty()
        || d.len() > 253
        || !d.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    {
        return None;
    }
    let name = brand_name_of_domain(&d)?;
    let squash = |t: &str| -> String {
        t.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase()
    };
    let hay = format!("{}{}{}", squash(label), squash(callout_title), squash(summary));
    if hay.contains(&squash(&name)) {
        Some(d)
    } else {
        None
    }
}

/// Optional per-item `stack_refs`: trimmed, lowercased, de-duplicated,
/// empties dropped, at most 4. Missing/malformed → empty. Pure.
fn parse_stack_refs(raw: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(arr) = raw.get("stack_refs").and_then(|v| v.as_array()) else {
        return out;
    };
    for entry in arr {
        let Some(s) = entry.as_str() else { continue };
        let s = s.trim().to_ascii_lowercase();
        if s.is_empty() || out.iter().any(|x| *x == s) {
            continue;
        }
        out.push(s);
        if out.len() >= 4 {
            break;
        }
    }
    out
}

/// Parse + validate a model response into a tour script: tolerant of fences,
/// prose and truncation; caps items, drops degenerate/duplicate boxes,
/// enforces every word budget deterministically, renumbers ids. `None` when
/// nothing usable survives (caller falls back to the legacy chain). Pure.
pub fn validate_script(text: &str) -> Option<TourScript> {
    let json_str = strip_json_fences(text);
    let (head, raw_items): (serde_json::Value, Vec<serde_json::Value>) =
        match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(v) => {
                let items = v.get("items").and_then(|i| i.as_array()).cloned();
                match items {
                    Some(items) => (v, items),
                    None => return None,
                }
            }
            Err(_) => salvage(text)?,
        };

    let mut items: Vec<TourItem> = Vec::new();
    // Per kept item: the model's id and the id it wants to link to. Links are
    // resolved only after renumbering, because items can be dropped.
    let mut raw_ids: Vec<u32> = Vec::new();
    let mut raw_links: Vec<Option<u32>> = Vec::new();
    for (ordinal, raw) in raw_items.iter().enumerate() {
        if items.len() >= MAX_ITEMS {
            break;
        }
        let Some(box_2d) = normalize_box(raw.get("box_2d").unwrap_or(&serde_json::Value::Null))
        else {
            continue;
        };
        if items.iter().any(|it| iou(&it.spatial.box_2d, &box_2d) > 0.7) {
            continue;
        }
        let label = str_field(raw, "label");
        let summary = str_field(raw, "summary");
        let spoken_raw = {
            let s = str_field(raw, "spoken");
            if !s.is_empty() {
                s
            } else if !summary.is_empty() {
                summary.clone()
            } else {
                label.clone()
            }
        };
        if spoken_raw.is_empty() && label.is_empty() {
            continue;
        }
        let label = if label.is_empty() {
            limit_words(&spoken_raw, CALLOUT_TITLE_MAX_WORDS, false)
        } else {
            label
        };
        let callout_title = {
            let t = str_field(raw, "callout_title");
            limit_words(if t.is_empty() { &label } else { &t }, CALLOUT_TITLE_MAX_WORDS, false)
        };
        let callout_text = {
            let t = str_field(raw, "callout_text");
            let src = if !t.is_empty() {
                t
            } else if !summary.is_empty() {
                summary.clone()
            } else {
                spoken_raw.clone()
            };
            limit_words(&src, CALLOUT_TEXT_MAX_WORDS, false)
        };
        let spoken = limit_words(&spoken_raw, SPOKEN_MAX_WORDS, true);
        let details: Vec<SpatialDetailRow> = raw
            .get("details")
            .and_then(|d| d.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        let title = r.get("title")?.as_str()?.trim().to_string();
                        let value = r.get("value")?.as_str()?.trim().to_string();
                        if title.is_empty() || value.is_empty() {
                            None
                        } else {
                            Some(SpatialDetailRow { title, value })
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let has_target = box_area_fraction(&box_2d) <= 0.8;
        let category = {
            let c = str_field(raw, "category");
            if c.is_empty() { "other".to_string() } else { c }
        };
        let draw = {
            let d = str_field(raw, "draw");
            let mut d = DrawStyle::parse(&d)
                .unwrap_or_else(|| DrawStyle::default_for_category(&category));
            let area = box_area_fraction(&box_2d);
            // A magnifier over most of the screen is meaningless; a
            // spotlight over most of it is not a spotlight.
            if d == DrawStyle::Lens && area > 0.30 {
                d = DrawStyle::Ring;
            }
            if d == DrawStyle::Spotlight && area > 0.60 {
                d = DrawStyle::Ring;
            }
            d
        };
        let raw_id = raw
            .get("id")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or((ordinal + 1) as u32);
        let raw_link = raw.get("link_to").and_then(|v| v.as_u64()).map(|v| v as u32);
        let label_for_brand = label.clone();
        let callout_title_for_brand = callout_title.clone();
        let summary_for_brand = summary.clone();
        items.push(TourItem {
            spatial: SpatialAnalysisItem {
                id: 0, // renumbered below
                label,
                category,
                box_2d,
                confidence: raw.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.9) as f32,
                summary: if summary.is_empty() { spoken.clone() } else { summary },
                details,
                image_data_url: None,
                logo_data_url: None,
            },
            spoken,
            callout_title,
            callout_text,
            has_target,
            draw,
            link_to: None, // resolved after renumbering
            thumb: None,
            stack_refs: parse_stack_refs(raw),
            brand: parse_brand(raw, &label_for_brand, &callout_title_for_brand, &summary_for_brand),
        });
        raw_ids.push(raw_id);
        raw_links.push(raw_link);
    }
    if items.is_empty() {
        return None;
    }

    let overview = {
        let o = str_field(&head, "overview");
        if o.is_empty() {
            FALLBACK_OVERVIEW.to_string()
        } else {
            limit_words(&o, OVERVIEW_MAX_WORDS, true)
        }
    };
    // Total spoken budget: drop trailing items (least important) first.
    let fixed = word_count(&overview) + word_count(CLOSER);
    while items.len() > 1
        && fixed + items.iter().map(|i| word_count(&i.spoken)).sum::<usize>()
            > MAX_TOTAL_SPOKEN_WORDS
    {
        items.pop();
    }
    for (n, it) in items.iter_mut().enumerate() {
        it.spatial.id = (n + 1) as u32;
    }
    raw_ids.truncate(items.len());
    raw_links.truncate(items.len());
    // Resolve `link_to` (model ids → 0-based indices after renumbering);
    // drop links to a missing item or to the item itself.
    for i in 0..items.len() {
        items[i].link_to = raw_links[i]
            .and_then(|target| raw_ids.iter().position(|id| *id == target))
            .filter(|j| *j != i);
    }
    // Keep at most one spotlight; a connect without a link is just a ring.
    let mut seen_spotlight = false;
    for it in items.iter_mut() {
        let mut draw = it.draw;
        if draw == DrawStyle::Spotlight {
            if seen_spotlight {
                draw = DrawStyle::Ring;
            } else {
                seen_spotlight = true;
            }
        }
        if draw == DrawStyle::Connect && it.link_to.is_none() {
            draw = DrawStyle::Ring;
        }
        it.draw = draw;
    }
    let intent = Intent::parse(&str_field(&head, "intent")).unwrap_or(Intent::Explain);
    let title = {
        let t = str_field(&head, "title");
        if t.is_empty() { "Screen Tour".to_string() } else { t }
    };
    // Written answer for the sidebar: keep line breaks/markdown, only cap length.
    let answer = {
        let a = str_field(&head, "answer");
        if word_count(&a) > ANSWER_MAX_WORDS {
            limit_words(&a, ANSWER_MAX_WORDS, false)
        } else {
            a
        }
    };
    Some(TourScript { title, intent, overview, answer, items, region: (0, 0, 0, 0) })
}

/// `validate_script`, then keep only `stack_refs` that name a detected
/// technology (exact match after lowercasing). Unknown refs are dropped, so
/// the overlay can never highlight a card that does not exist. Pure.
pub fn validate_script_for_stack(text: &str, known_ids: &[String]) -> Option<TourScript> {
    let mut script = validate_script(text)?;
    let known: Vec<String> = known_ids
        .iter()
        .map(|id| id.trim().to_ascii_lowercase())
        .filter(|id| !id.is_empty())
        .collect();
    for item in script.items.iter_mut() {
        item.stack_refs.retain(|r| known.iter().any(|k| k == r));
    }
    Some(script)
}

// ─── Narration plan ────────────────────────────────────────────────────

/// One spoken line; `item` is the callout shown while it plays (None for the
/// overview and the closer — the screen is clear during those).
#[derive(Debug, Clone, PartialEq)]
pub struct NarrStep {
    pub text: String,
    pub item: Option<usize>,
}

/// overview → item 0..n → closer. Pure.
pub fn narration_steps(script: &TourScript) -> Vec<NarrStep> {
    let mut steps = vec![NarrStep { text: script.overview.clone(), item: None }];
    for (i, it) in script.items.iter().enumerate() {
        steps.push(NarrStep { text: it.spoken.clone(), item: Some(i) });
    }
    steps.push(NarrStep { text: CLOSER.to_string(), item: None });
    steps
}

/// Timed dwell for a line when there is no audio (offline / TTS failed):
/// the callout text carries the content, so show it long enough to read.
pub fn dwell_ms(words: usize) -> u64 {
    (words as u64 * 330 + 1200).clamp(2500, 9000)
}

// ─── One-callout-at-a-time state machine ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Done,
    Cancelled,
    #[allow(dead_code)]
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Draw the callout for item index.
    Show(usize),
    /// Remove the currently drawn callout.
    Clear,
    /// Tour over (emitted exactly once, nothing after).
    End(EndReason),
}

/// Drives the overlay from narration events (or a timer when silent).
/// Invariants (tested): `Show` is only ever emitted while nothing is shown
/// (a `Clear` always precedes it); nothing is emitted after `End`; `End` is
/// emitted exactly once. Injected clock — no real time inside.
#[derive(Debug)]
pub struct TourEngine {
    item_for_step: Vec<Option<usize>>,
    words_for_step: Vec<usize>,
    showing: bool,
    current_step: Option<usize>,
    next_step: usize,
    ended: bool,
    /// Some(deadline_ms) once timed (silent) mode is active.
    timed_deadline: Option<u64>,
}

impl TourEngine {
    pub fn new(steps: &[NarrStep]) -> Self {
        Self {
            item_for_step: steps.iter().map(|s| s.item).collect(),
            words_for_step: steps.iter().map(|s| word_count(&s.text)).collect(),
            showing: false,
            current_step: None,
            next_step: 0,
            ended: false,
            timed_deadline: None,
        }
    }

    fn clear_if_showing(&mut self, out: &mut Vec<Action>) {
        if self.showing {
            self.showing = false;
            out.push(Action::Clear);
        }
    }

    /// Line `i` became audible.
    pub fn step_started(&mut self, i: usize) -> Vec<Action> {
        let mut out = Vec::new();
        if self.ended || i >= self.item_for_step.len() || i < self.next_step {
            return out; // duplicate / stale / out of range
        }
        // Jumping forward (a lost event) still clears first.
        self.clear_if_showing(&mut out);
        self.current_step = Some(i);
        self.next_step = i + 1;
        if let Some(item) = self.item_for_step[i] {
            self.showing = true;
            out.push(Action::Show(item));
        }
        out
    }

    /// Line `i` finished playing.
    pub fn step_ended(&mut self, i: usize) -> Vec<Action> {
        let mut out = Vec::new();
        if self.ended || self.current_step != Some(i) {
            return out;
        }
        self.clear_if_showing(&mut out);
        self.current_step = None;
        if i + 1 >= self.item_for_step.len() {
            self.ended = true;
            out.push(Action::End(EndReason::Done));
        }
        out
    }

    /// No audio will play: switch to timed dwell starting now.
    pub fn audio_unavailable(&mut self, now_ms: u64) -> Vec<Action> {
        if self.ended || self.timed_deadline.is_some() {
            return Vec::new();
        }
        let first = self.next_step;
        let mut out = self.step_started(first);
        self.timed_deadline = Some(now_ms + dwell_ms(self.words_for_step.get(first).copied().unwrap_or(0)));
        if self.item_for_step.is_empty() {
            self.ended = true;
            out.push(Action::End(EndReason::Done));
        }
        out
    }

    /// Advance timed dwell (no-op unless `audio_unavailable` was called).
    pub fn tick(&mut self, now_ms: u64) -> Vec<Action> {
        let Some(deadline) = self.timed_deadline else { return Vec::new() };
        if self.ended || now_ms < deadline {
            return Vec::new();
        }
        let Some(cur) = self.current_step else { return Vec::new() };
        let mut out = self.step_ended(cur);
        if !self.ended {
            let next = cur + 1;
            out.extend(self.step_started(next));
            self.timed_deadline = Some(now_ms + dwell_ms(self.words_for_step.get(next).copied().unwrap_or(0)));
        }
        out
    }

    /// Barge-in / Esc / "stop" / failure. Idempotent.
    pub fn end(&mut self, reason: EndReason) -> Vec<Action> {
        let mut out = Vec::new();
        if self.ended {
            return out;
        }
        self.clear_if_showing(&mut out);
        self.ended = true;
        out.push(Action::End(reason));
        out
    }

    pub fn is_ended(&self) -> bool {
        self.ended
    }
}

// ─── Routing predicate ─────────────────────────────────────────────────

/// Should this screen-analysis prompt get the narrated tour (vision)?
/// Analyse/explain/describe/why/compare… (the `Visual` tier) and the
/// "what am I seeing / what is on my screen" family → tour. Pure
/// transcription requests ("read / scan / copy my screen") stay on the free
/// OCR path unless they also ask to analyse/explain. Pure.
pub fn wants_narrated_tour(prompt: &str) -> bool {
    let p = prompt.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| p.contains(n));
    let analyse = has(&["analys", "analyz", "explain", "describe", "summar", "diagnos", "compar", "teach"]);
    let transcribe = has(&["read ", "scan", "transcribe", "copy", "extract", "list the text"]);
    if transcribe && !analyse {
        return false;
    }
    if crate::orchestrator::classify_screen_query(prompt)
        == crate::orchestrator::ScreenQueryTier::Visual
    {
        return true;
    }
    has(&[
        "what am i seeing",
        "what am i looking at",
        "what can i see",
        "what do i see",
        "what i see",
        "what i can see",
        "what is on my screen",
        "what's on my screen",
        "whats on my screen",
        "what is on the screen",
        "what's on the screen",
        "what is on this screen",
    ])
}

/// `screenTour` setting (default ON). Pure over the settings JSON text.
pub fn setting_enabled(settings_json: Option<&str>) -> bool {
    settings_json
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("screenTour").and_then(|b| b.as_bool()))
        .unwrap_or(true)
}

pub fn tour_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let text = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok());
    setting_enabled(text.as_deref())
}

// ─── Overlay payloads ──────────────────────────────────────────────────

/// `screen:callout` payload — physical SCREEN px (the frontend divides by
/// dpr, the same contract as `ghost:ring`). The model's 0–1000 box is
/// relative to the captured `region` (cropped page content or the whole
/// monitor), so it is scaled to the region and offset by its origin. Only the
/// CURRENT item is ever sent. `link` is the same mapping for the item this
/// one connects to (null when there is no valid link); `thumb` is included
/// only for a `Lens` item.
pub fn callout_json(
    request_id: &str,
    script: &TourScript,
    idx: usize,
    region: (i32, i32, i32, i32),
) -> serde_json::Value {
    let Some(item) = script.items.get(idx) else {
        return serde_json::Value::Null;
    };
    let (rx, ry, rw, rh) = region;
    let map_box = |b: &SpatialBoundingBox| {
        let (bx, by, w, h) = denormalize_spatial_box(b, rw, rh);
        serde_json::json!({
            "x": rx + bx, "y": ry + by, "width": w, "height": h
        })
    };
    let (x, y, w, h) = {
        let (bx, by, bw, bh) = denormalize_spatial_box(&item.spatial.box_2d, rw, rh);
        (rx + bx, ry + by, bw, bh)
    };
    let link = item
        .link_to
        .and_then(|j| script.items.get(j))
        .map(|target| map_box(&target.spatial.box_2d));
    let thumb = if item.draw == DrawStyle::Lens {
        item.thumb.clone()
    } else {
        None
    };
    serde_json::json!({
        "request_id": request_id,
        "idx": idx,
        "total": script.items.len(),
        "title": item.callout_title,
        "text": item.callout_text,
        "category": item.spatial.category,
        "x": x, "y": y, "width": w, "height": h,
        "has_target": item.has_target,
        "draw": item.draw.as_str(),
        "intent": script.intent.as_str(),
        "link": link,
        "thumb": thumb,
        "stack_refs": item.stack_refs,
    })
}

// ─── Gemini glue ───────────────────────────────────────────────────────

/// `tourModel` from settings JSON text (empty/absent → None). Pure.
pub fn tour_model_setting(settings_json: Option<&str>) -> Option<String> {
    settings_json
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("tourModel").and_then(|m| m.as_str().map(|s| s.trim().to_string())))
        .filter(|m| !m.is_empty())
}

/// Output budget for the tour call. Stronger models spend part of
/// `maxOutputTokens` on thinking, so this is generous — a truncated JSON
/// answer would cost items (`validate_script` salvages what is complete).
const TOUR_MAX_OUTPUT_TOKENS: u32 = 8192;

/// Whole-ladder wall-clock cap (each request also has its own timeout).
/// The old 6 s clamp made the tour impossible (a tour answer needs 6-20 s),
/// so the whole ladder may take up to 25 s; each request is capped at
/// `vision::TOUR_REQUEST_TIMEOUT_SECS`.
pub const TOUR_TOTAL_TIMEOUT_SECS: u64 = 25;

/// Speak one reassurance line if the model has not answered by then.
pub const SLOW_NOTICE_AFTER_SECS: u64 = 6;

/// Shared Gemini pass for a tour script: key/quota gates, model ladder, own
/// pooled client, whole-ladder timeout, quota bookkeeping, `region` stamped on
/// the script. The caller supplies the prompt and the validator.
async fn analyze_tour_script<R: Runtime>(
    app: &AppHandle<R>,
    prompt: &str,
    b64: &str,
    region: (i32, i32, i32, i32),
    parse: impl Fn(&str) -> Option<TourScript>,
) -> Result<TourScript, String> {
    let key = crate::commands::read_api_key(app, "gemini");
    if key.is_empty() {
        return Err("No Gemini key configured".to_string());
    }
    let usage_dir = app
        .path()
        .app_data_dir()
        .map_err(|e: tauri::Error| e.to_string())?;
    if crate::vision::exhausted(&usage_dir, "gemini") {
        return Err("Gemini quota exhausted".to_string());
    }
    let settings = std::fs::read_to_string(usage_dir.join("settings.json")).ok();
    let models = crate::vision::tour_model_ladder(tour_model_setting(settings.as_deref()).as_deref());
    // The tour needs a long per-request timeout (a tour answer needs 6-20 s),
    // so it runs on its own pooled client — click grounding keeps the fast 4 s one.
    let client = crate::vision::tour_vision_client();
    let call = crate::vision::gemini_json_with_image(
        prompt,
        b64,
        &key,
        &client,
        TOUR_MAX_OUTPUT_TOKENS,
        &models,
        parse,
    );
    let step = tokio::time::timeout(std::time::Duration::from_secs(TOUR_TOTAL_TIMEOUT_SECS), call)
        .await
        .map_err(|_| "Vision model timed out".to_string())?;
    match step {
        crate::vision::JsonStep::Found(mut script) => {
            script.region = region;
            crate::vision::record_use(&usage_dir, "gemini");
            tracing::info!("screen_tour: script ready ({} items)", script.items.len());
            Ok(script)
        }
        crate::vision::JsonStep::Quota => {
            crate::vision::mark_exhausted(&usage_dir, "gemini");
            Err("Gemini quota exhausted".to_string())
        }
        crate::vision::JsonStep::Miss => Err("No usable tour script from the vision model".to_string()),
    }
}

/// Ask Gemini for the tour script for an already-captured image of `region`
/// (screen px). Strong model first, lite fallback (see
/// `vision::tour_model_ladder`); shared quota ledger. The returned script's
/// `region` is set so boxes map back to real screen pixels.
pub async fn analyze_tour_image<R: Runtime>(
    app: &AppHandle<R>,
    user_prompt: &str,
    context_block: &str,
    b64: &str,
    region: (i32, i32, i32, i32),
) -> Result<TourScript, String> {
    let prompt = build_tour_prompt(user_prompt, context_block);
    analyze_tour_script(app, &prompt, b64, region, validate_script).await
}

/// Reverse-engineer pass: same Gemini flow as `analyze_tour_image`, with the
/// section prompt and a stack-aware validator (refs outside `techs` are
/// dropped). `region` still maps the model's boxes back to screen pixels.
pub async fn analyze_reverse_image<R: Runtime>(
    app: &AppHandle<R>,
    user_prompt: &str,
    context_block: &str,
    stack_brief: &str,
    techs: &[(String, String)],
    viewport_idx: usize,
    viewport_total_hint: usize,
    prior_sections: &[String],
    b64: &str,
    region: (i32, i32, i32, i32),
) -> Result<TourScript, String> {
    let prompt = build_reverse_prompt(
        user_prompt,
        context_block,
        stack_brief,
        techs,
        viewport_idx,
        viewport_total_hint,
        prior_sections,
    );
    let ids: Vec<String> = techs.iter().map(|(id, _)| id.clone()).collect();
    analyze_tour_script(app, &prompt, b64, region, move |text| {
        validate_script_for_stack(text, &ids)
    })
    .await
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_notice_comes_before_the_cap() {
        // The reassurance line must fire while we are still waiting on the
        // model, and the ladder's total cap must give each request room.
        assert!(SLOW_NOTICE_AFTER_SECS < TOUR_TOTAL_TIMEOUT_SECS);
        assert!(crate::vision::TOUR_REQUEST_TIMEOUT_SECS <= TOUR_TOTAL_TIMEOUT_SECS);
        assert!(TOUR_TOTAL_TIMEOUT_SECS >= 20);
    }

    fn good_json() -> String {
        r#"{"title":"Dry Fruits Shop","overview":"A product page listing three dry fruits with prices.",
        "items":[
          {"id":1,"label":"Almonds","category":"product","box_2d":[100,50,400,300],"confidence":0.95,
           "spoken":"These are almonds priced at four hundred rupees.","callout_title":"Almonds",
           "callout_text":"400 rupees per 500 grams","summary":"Raw almonds.",
           "details":[{"title":"Price","value":"400"},{"title":"","value":"x"}]},
          {"id":2,"label":"Cashews","category":"product","box_2d":[100,350,400,650],
           "spoken":"Cashews are next.","summary":"Whole cashews, 600 rupees."},
          {"id":3,"label":"Checkout","category":"ui_element","box_2d":[800,700,950,950],
           "spoken":"The checkout button is at the bottom right."}
        ]}"#
            .to_string()
    }

    #[test]
    fn validates_clean_json() {
        let s = validate_script(&good_json()).unwrap();
        assert_eq!(s.title, "Dry Fruits Shop");
        assert_eq!(s.items.len(), 3);
        assert_eq!(s.items[0].spatial.id, 1);
        assert_eq!(s.items[2].spatial.id, 3);
        assert_eq!(s.items[0].spatial.details.len(), 1); // blank row dropped
        assert!(s.items.iter().all(|i| i.has_target));
        // missing callout_text falls back to summary
        assert_eq!(s.items[1].callout_text, "Whole cashews, 600 rupees.");
        // missing summary falls back to the spoken line
        assert_eq!(s.items[2].spatial.summary, s.items[2].spoken);
    }

    #[test]
    fn validates_fenced_and_prose_wrapped() {
        let fenced = format!("```json\n{}\n```", good_json());
        assert_eq!(validate_script(&fenced).unwrap().items.len(), 3);
        let prose = format!("Sure! Here you go: {} Hope it helps.", good_json());
        assert_eq!(validate_script(&prose).unwrap().items.len(), 3);
    }

    #[test]
    fn salvages_truncated_json() {
        let full = good_json();
        // Cut inside the third item.
        let cut = full.find("\"id\":3").unwrap() + 20;
        let s = validate_script(&full[..cut]).unwrap();
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.title, "Dry Fruits Shop");
        assert!(s.overview.starts_with("A product page"));
    }

    #[test]
    fn truncated_without_complete_item_is_none() {
        let raw = r#"{"title":"T","items":[{"label":"A","box_2d":[10,10,"#;
        assert!(validate_script(raw).is_none());
    }

    #[test]
    fn rejects_garbage_and_empty() {
        assert!(validate_script("sorry I cannot").is_none());
        assert!(validate_script("{\"items\":[]}").is_none());
        assert!(validate_script("{\"title\":\"x\"}").is_none());
    }

    #[test]
    fn degenerate_and_duplicate_boxes_dropped() {
        let raw = r#"{"items":[
          {"label":"Dot","box_2d":[10,10,15,15],"spoken":"tiny"},
          {"label":"A","box_2d":[100,100,300,400],"spoken":"First."},
          {"label":"A again","box_2d":[102,101,299,402],"spoken":"Dup."},
          {"label":"B","box_2d":[500,500,700,800],"spoken":"Second."}]}"#;
        let s = validate_script(raw).unwrap();
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.items[0].spatial.label, "A");
        assert_eq!(s.items[1].spatial.label, "B");
        assert_eq!(s.items[1].spatial.id, 2);
    }

    #[test]
    fn full_screen_box_loses_target_but_keeps_callout() {
        let raw = r#"{"items":[{"label":"Whole page","box_2d":[0,0,1000,1000],"spoken":"A web page."}]}"#;
        let s = validate_script(raw).unwrap();
        assert!(!s.items[0].has_target);
        assert_eq!(s.items.len(), 1);
    }

    #[test]
    fn caps_items_at_max() {
        let mut items = String::new();
        for i in 0..9 {
            let y = i * 100;
            items.push_str(&format!(
                "{{\"label\":\"I{i}\",\"box_2d\":[{y},0,{},300],\"spoken\":\"Item {i}.\"}},",
                y + 60
            ));
        }
        let raw = format!("{{\"items\":[{}]}}", items.trim_end_matches(','));
        assert_eq!(validate_script(&raw).unwrap().items.len(), MAX_ITEMS);
    }

    #[test]
    fn word_budgets_enforced() {
        let long: String = (0..60).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let raw = format!(
            "{{\"overview\":\"{long}\",\"items\":[{{\"label\":\"A\",\"callout_title\":\"{long}\",\
\"callout_text\":\"{long}\",\"box_2d\":[100,100,300,400],\"spoken\":\"{long}\"}}]}}"
        );
        let s = validate_script(&raw).unwrap();
        assert!(word_count(&s.overview) <= OVERVIEW_MAX_WORDS + 0);
        assert!(word_count(&s.items[0].spoken) <= SPOKEN_MAX_WORDS);
        assert!(word_count(&s.items[0].callout_title) <= CALLOUT_TITLE_MAX_WORDS);
        assert!(word_count(&s.items[0].callout_text) <= CALLOUT_TEXT_MAX_WORDS);
        assert!(s.items[0].spoken.ends_with('.'));
    }

    #[test]
    fn total_spoken_budget_drops_trailing_items() {
        // 5 items x 22 words + overview 35 + closer 7 = 152 > 120 → trims tail.
        let w22: String = (0..SPOKEN_MAX_WORDS).map(|i| format!("a{i}")).collect::<Vec<_>>().join(" ");
        let w35: String = (0..OVERVIEW_MAX_WORDS).map(|i| format!("o{i}")).collect::<Vec<_>>().join(" ");
        let mut items = String::new();
        for i in 0..5 {
            let y = i * 190;
            items.push_str(&format!(
                "{{\"label\":\"I{i}\",\"box_2d\":[{y},0,{},300],\"spoken\":\"{w22}\"}},",
                y + 150
            ));
        }
        let raw = format!("{{\"overview\":\"{w35}\",\"items\":[{}]}}", items.trim_end_matches(','));
        let s = validate_script(&raw).unwrap();
        let total = word_count(&s.overview)
            + word_count(CLOSER)
            + s.items.iter().map(|i| word_count(&i.spoken)).sum::<usize>();
        assert!(total <= MAX_TOTAL_SPOKEN_WORDS, "total {total}");
        assert!(s.items.len() < 5 && !s.items.is_empty());
        assert_eq!(s.items.last().unwrap().spatial.id as usize, s.items.len());
    }

    #[test]
    fn limit_words_prefers_sentence_end() {
        let s = limit_words("This is one. And then a very long rambling second sentence here", 6, false);
        assert_eq!(s, "This is one.");
        assert_eq!(limit_words("a b c d", 10, true), "a b c d.");
        assert_eq!(limit_words("", 5, true), "");
        assert_eq!(limit_words("one two three, four five", 3, false), "one two three");
    }

    #[test]
    fn missing_overview_gets_honest_fallback() {
        let raw = r#"{"items":[{"label":"A","box_2d":[100,100,300,400],"spoken":"Hi."}]}"#;
        assert_eq!(validate_script(raw).unwrap().overview, FALLBACK_OVERVIEW);
    }

    #[test]
    fn narration_plan_shape() {
        let s = validate_script(&good_json()).unwrap();
        let steps = narration_steps(&s);
        assert_eq!(steps.len(), s.items.len() + 2);
        assert_eq!(steps[0].item, None);
        assert_eq!(steps[1].item, Some(0));
        assert_eq!(steps.last().unwrap().text, CLOSER);
        assert_eq!(steps.last().unwrap().item, None);
    }

    #[test]
    fn dwell_clamps() {
        assert_eq!(dwell_ms(0), 2500);
        assert_eq!(dwell_ms(10), 4500);
        assert_eq!(dwell_ms(500), 9000);
    }

    #[test]
    fn payload_matches_cards() {
        let s = validate_script(&good_json()).unwrap();
        let p = s.to_payload();
        assert_eq!(p.items.len(), s.items.len());
        assert_eq!(p.items[1].id, 2);
        assert_eq!(p.provider_used, "gemini");
    }

    #[test]
    fn callout_json_is_physical_px_current_only() {
        let s = validate_script(&good_json()).unwrap();
        let j = callout_json("r1", &s, 0, (0, 0, 1920, 1080));
        assert_eq!(j["idx"], 0);
        assert_eq!(j["x"], 96); // 50/1000 * 1920
        assert_eq!(j["y"], 108); // 100/1000 * 1080
        assert_eq!(j["width"], 480);
        assert_eq!(j["has_target"], true);
        assert_eq!(j["title"], "Almonds");
        assert_eq!(j["category"], "product");
    }

    #[test]
    fn callout_json_maps_a_cropped_region_back_to_screen_px() {
        // Page content only: 1920x950 at (0, 82) — the browser chrome was cut off.
        let s = validate_script(&good_json()).unwrap();
        let j = callout_json("r1", &s, 0, (0, 82, 1920, 950));
        assert_eq!(j["x"], 96); // 50/1000 * 1920 + 0
        assert_eq!(j["y"], 82 + 95); // 100/1000 * 950 = 95, offset by the 82-px toolbar
        assert_eq!(j["width"], 480);
        assert_eq!(j["height"], 285); // 300/1000 * 950

        // A region that does not start at the origin on x either.
        let j2 = callout_json("r1", &s, 0, (460, 180, 1000, 600));
        assert_eq!(j2["x"], 460 + 50); // 50/1000 * 1000
        assert_eq!(j2["y"], 180 + 60); // 100/1000 * 600
    }

    #[test]
    fn answer_becomes_the_sidebar_overview_but_not_the_spoken_one() {
        let raw = r#"{"title":"Healthiest nuts","overview":"Seven kinds of nuts and seeds are shown.",
            "answer":"Seven kinds: pumpkin seeds, hazelnuts, sunflower seeds, cashews, pistachios, walnuts and almonds.",
            "items":[{"label":"Walnuts","box_2d":[100,100,300,400],"spoken":"Walnuts are the richest in omega-3."}]}"#;
        let s = validate_script(raw).unwrap();
        assert!(s.overview.starts_with("Seven kinds of nuts"));
        assert!(s.answer.starts_with("Seven kinds:"));
        assert_eq!(s.to_payload().overview, s.answer);
        // No answer → the sidebar falls back to the spoken overview.
        let no_answer = r#"{"overview":"A page.","items":[{"label":"A","box_2d":[100,100,300,400],"spoken":"Hi."}]}"#;
        let s2 = validate_script(no_answer).unwrap();
        assert_eq!(s2.to_payload().overview, s2.overview);
    }

    #[test]
    fn answer_is_length_capped_but_keeps_its_text() {
        let long: String = (0..400).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let raw = format!(
            "{{\"answer\":\"{long}\",\"items\":[{{\"label\":\"A\",\"box_2d\":[100,100,300,400],\"spoken\":\"Hi.\"}}]}}"
        );
        let s = validate_script(&raw).unwrap();
        assert!(word_count(&s.answer) <= ANSWER_MAX_WORDS);
        assert!(s.answer.starts_with("w0 w1"));
    }

    #[test]
    fn tour_model_setting_parsing() {
        assert_eq!(tour_model_setting(None), None);
        assert_eq!(tour_model_setting(Some("{}")), None);
        assert_eq!(tour_model_setting(Some("{\"tourModel\": \"  \"}")), None);
        assert_eq!(
            tour_model_setting(Some("{\"tourModel\": \"gemini-3.8-flash\"}")).as_deref(),
            Some("gemini-3.8-flash")
        );
        assert_eq!(tour_model_setting(Some("not json")), None);
    }

    // ── Engine ──

    fn engine(n_items: usize) -> (TourEngine, Vec<NarrStep>) {
        let mut steps = vec![NarrStep { text: "Overview here.".into(), item: None }];
        for i in 0..n_items {
            steps.push(NarrStep { text: format!("Item {i} spoken now."), item: Some(i) });
        }
        steps.push(NarrStep { text: CLOSER.into(), item: None });
        (TourEngine::new(&steps), steps)
    }

    #[test]
    fn engine_happy_path() {
        let (mut e, steps) = engine(2);
        let mut log = Vec::new();
        for i in 0..steps.len() {
            log.extend(e.step_started(i));
            log.extend(e.step_ended(i));
        }
        assert_eq!(
            log,
            vec![
                Action::Show(0), Action::Clear,
                Action::Show(1), Action::Clear,
                Action::End(EndReason::Done)
            ]
        );
        assert!(e.is_ended());
    }

    #[test]
    fn engine_ignores_duplicates_and_stale_events() {
        let (mut e, _) = engine(2);
        assert!(e.step_started(0).is_empty()); // overview: no callout
        assert!(e.step_started(0).is_empty()); // duplicate
        assert_eq!(e.step_started(1), vec![Action::Show(0)]);
        assert!(e.step_started(1).is_empty()); // duplicate
        assert!(e.step_started(0).is_empty()); // stale
        assert!(e.step_ended(0).is_empty()); // not current
        assert_eq!(e.step_ended(1), vec![Action::Clear]);
        assert!(e.step_ended(1).is_empty()); // duplicate end
        assert!(e.step_started(99).is_empty()); // out of range
    }

    #[test]
    fn engine_jump_forward_clears_first() {
        let (mut e, _) = engine(2);
        e.step_started(1);
        // step 1's end event was lost; step 2 starts.
        assert_eq!(e.step_started(2), vec![Action::Clear, Action::Show(1)]);
    }

    #[test]
    fn engine_cancel_in_every_state_ends_once() {
        for upto in 0..5usize {
            let (mut e, steps) = engine(2);
            for i in 0..upto.min(steps.len()) {
                e.step_started(i);
                if i + 1 < upto {
                    e.step_ended(i);
                }
            }
            if e.is_ended() {
                continue;
            }
            let first = e.end(EndReason::Cancelled);
            assert_eq!(first.last(), Some(&Action::End(EndReason::Cancelled)));
            assert!(e.end(EndReason::Cancelled).is_empty());
            assert!(e.step_started(3).is_empty());
            assert!(e.tick(1_000_000).is_empty());
        }
    }

    #[test]
    fn engine_timed_mode_walks_all_steps() {
        let (mut e, steps) = engine(2);
        let mut log = e.audio_unavailable(0);
        let mut t = 0u64;
        let mut guard = 0;
        while !e.is_ended() && guard < 100 {
            t += 500;
            log.extend(e.tick(t));
            guard += 1;
        }
        assert!(e.is_ended());
        // Overview shows nothing; items show then clear; ends once.
        assert_eq!(log.iter().filter(|a| matches!(a, Action::Show(_))).count(), 2);
        assert_eq!(log.iter().filter(|a| matches!(a, Action::End(_))).count(), 1);
        assert_eq!(log.last(), Some(&Action::End(EndReason::Done)));
        let _ = steps;
    }

    #[test]
    fn engine_timed_mode_respects_dwell() {
        let (mut e, _) = engine(1);
        e.audio_unavailable(0);
        assert!(e.tick(2499).is_empty()); // overview dwell is >= 2500
        assert!(!e.tick(2500).is_empty() || !e.is_ended());
    }

    #[test]
    fn engine_never_shows_over_a_visible_callout() {
        // Pseudo-random event soup: simulate the visual state and assert the
        // one-at-a-time invariant on every emitted action.
        let mut seed = 99u64;
        for _round in 0..200 {
            let (mut e, steps) = engine(3);
            let mut visible = false;
            let mut ended = false;
            let n = steps.len();
            for _ in 0..60 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let r = (seed >> 33) as usize;
                let i = r % (n + 1);
                let acts = match r % 5 {
                    0 | 1 => e.step_started(i),
                    2 | 3 => e.step_ended(i),
                    _ => {
                        if r % 7 == 0 { e.end(EndReason::Cancelled) } else { e.tick((r % 20_000) as u64) }
                    }
                };
                for a in acts {
                    assert!(!ended, "action after End: {a:?}");
                    match a {
                        Action::Show(_) => {
                            assert!(!visible, "Show while a callout is visible");
                            visible = true;
                        }
                        Action::Clear => {
                            assert!(visible, "Clear with nothing visible");
                            visible = false;
                        }
                        Action::End(_) => {
                            assert!(!visible, "End while a callout is visible");
                            ended = true;
                        }
                    }
                }
            }
        }
    }

    // ── Routing ──

    #[test]
    fn tour_routing_positives() {
        for p in [
            "analyse my screen",
            "analyze the screen for me",
            "explain what i can see",
            "explain what i see",
            "what am i seeing",
            "what is on my screen",
            "what's on my screen",
            "what do i see",
            "describe my screen",
            "why is there an error on my screen",
        ] {
            assert!(wants_narrated_tour(p), "should tour: {p}");
        }
    }

    #[test]
    fn tour_routing_keeps_pure_transcription_on_ocr() {
        for p in [
            "read my screen",
            "scan my screen",
            "check my screen",
            "copy the text on my screen",
            "read what is on my screen",
        ] {
            assert!(!wants_narrated_tour(p), "should stay OCR: {p}");
        }
        // …unless the user also asks for analysis.
        assert!(wants_narrated_tour("read my screen and explain it"));
    }

    #[test]
    fn setting_defaults_on() {
        assert!(setting_enabled(None));
        assert!(setting_enabled(Some("{}")));
        assert!(setting_enabled(Some("not json")));
        assert!(!setting_enabled(Some("{\"screenTour\": false}")));
        assert!(setting_enabled(Some("{\"screenTour\": true}")));
    }

    #[test]
    fn acks_are_short_and_stable() {
        for s in 0..=255u8 {
            assert!(SCREEN_ACKS.contains(&screen_ack(s)));
        }
        assert!(SCREEN_ACKS.iter().all(|a| word_count(a) <= 6));
    }

    #[test]
    fn prompt_contract() {
        let ctx = "Context about what the user is looking at:\n- Window / tab title: Top 10 healthiest nuts | Women's Health";
        let p = build_tour_prompt("analyse my screen", ctx);
        for needle in [
            "analyse my screen",
            "box_2d",
            "0-1000",
            "spoken",
            "callout_text",
            "details",
            "\"answer\"",
            "Top 10 healthiest nuts", // the context block is embedded
        ] {
            assert!(p.contains(needle), "prompt missing {needle}");
        }
    }

    #[test]
    fn prompt_is_answer_first_ignores_chrome_and_requires_grounded_people() {
        let p = build_tour_prompt("what am i seeing", "ctx");
        let lower = p.to_lowercase();
        // Answer the implicit question and enumerate.
        assert!(lower.contains("implicit question"));
        assert!(lower.contains("name them") || lower.contains("name them."));
        // Not the browser chrome.
        assert!(lower.contains("never the browser"));
        assert!(lower.contains("address bar"));
        // Text can be quoted; a face alone cannot establish identity.
        assert!(lower.contains("do not infer who someone is from their face"));
        assert!(lower.contains("person 1"));
        assert!(lower.contains("never invent a biography"));
        assert!(lower.contains("vary sentence openings"));
        // Budgets are stated numerically.
        assert!(p.contains(&OVERVIEW_MAX_WORDS.to_string()));
        assert!(p.contains(&SPOKEN_MAX_WORDS.to_string()));
        assert!(p.contains(&ANSWER_MAX_WORDS.to_string()));
    }

    // ── Draw style & intent ──

    #[test]
    fn draw_style_round_trip_and_category_defaults() {
        for name in [
            "ring", "underline", "bracket", "circle", "arrow", "highlight", "spotlight", "scan",
            "lens", "connect", "pin",
        ] {
            let d = DrawStyle::parse(name).unwrap_or_else(|| panic!("parse {name}"));
            assert_eq!(d.as_str(), name);
            assert_eq!(DrawStyle::parse(&format!("  {}  ", name.to_uppercase())), Some(d));
        }
        assert_eq!(DrawStyle::parse("blob"), None);
        assert_eq!(DrawStyle::parse(""), None);
        assert_eq!(DrawStyle::default(), DrawStyle::Ring);
        for c in ["person", "product", "food", "object", "other", ""] {
            assert_eq!(DrawStyle::default_for_category(c), DrawStyle::Ring, "{c}");
        }
        assert_eq!(DrawStyle::default_for_category("ui_element"), DrawStyle::Arrow);
        assert_eq!(DrawStyle::default_for_category("text_block"), DrawStyle::Highlight);
        assert_eq!(DrawStyle::default_for_category("code"), DrawStyle::Highlight);
        assert_eq!(DrawStyle::default_for_category("chart"), DrawStyle::Bracket);
        assert_eq!(DrawStyle::default_for_category("diagram"), DrawStyle::Bracket);
    }

    #[test]
    fn intent_parse_is_tolerant() {
        assert_eq!(Intent::default(), Intent::Explain);
        assert_eq!(Intent::parse("Summarise"), Some(Intent::Summarize));
        assert_eq!(Intent::parse(" summary "), Some(Intent::Summarize));
        assert_eq!(Intent::parse("DEBUGGING"), Some(Intent::Debug));
        assert_eq!(Intent::parse("explaining"), Some(Intent::Explain));
        assert_eq!(Intent::parse("comparison"), Some(Intent::Compare));
        assert_eq!(Intent::parse("locate"), Some(Intent::Find));
        assert_eq!(Intent::parse("recommend"), Some(Intent::Decide));
        assert_eq!(Intent::parse("???"), None);
        for i in [
            Intent::Explain,
            Intent::Find,
            Intent::Compare,
            Intent::Summarize,
            Intent::Decide,
            Intent::Debug,
        ] {
            assert_eq!(Intent::parse(i.as_str()), Some(i));
        }
    }

    #[test]
    fn connect_and_link_survive_validation() {
        let raw = r#"{"intent":"compare","items":[
          {"id":1,"label":"A","category":"product","box_2d":[100,100,300,400],"spoken":"A.","draw":"connect","link_to":2},
          {"id":2,"label":"B","category":"product","box_2d":[500,500,700,800],"spoken":"B."}]}"#;
        let s = validate_script(raw).unwrap();
        assert_eq!(s.intent, Intent::Compare);
        assert_eq!(s.items[0].draw, DrawStyle::Connect);
        assert_eq!(s.items[0].link_to, Some(1));
        // No draw on item 2 → the category default.
        assert_eq!(s.items[1].draw, DrawStyle::Ring);
    }

    #[test]
    fn connect_without_link_becomes_ring_and_self_links_are_dropped() {
        let raw = r#"{"items":[
          {"id":1,"label":"A","box_2d":[100,100,300,400],"spoken":"A.","draw":"connect"},
          {"id":2,"label":"B","box_2d":[500,500,700,800],"spoken":"B.","draw":"connect","link_to":2}]}"#;
        let s = validate_script(raw).unwrap();
        assert_eq!(s.items[0].draw, DrawStyle::Ring);
        assert_eq!(s.items[0].link_to, None);
        assert_eq!(s.items[1].draw, DrawStyle::Ring);
        assert_eq!(s.items[1].link_to, None);
    }

    #[test]
    fn lens_on_a_large_box_and_extra_spotlights_fall_back_to_ring() {
        let raw = r#"{"items":[
          {"label":"Big","box_2d":[0,0,600,1000],"spoken":"Big.","draw":"lens"},
          {"label":"S1","box_2d":[100,100,300,300],"spoken":"S1.","draw":"spotlight"},
          {"label":"S2","box_2d":[500,500,700,700],"spoken":"S2.","draw":"spotlight"}]}"#;
        let s = validate_script(raw).unwrap();
        assert_eq!(s.items[0].draw, DrawStyle::Ring);
        assert_eq!(s.items[1].draw, DrawStyle::Spotlight);
        assert_eq!(s.items[2].draw, DrawStyle::Ring);
        // A spotlight over more than 60 % of the region is not a spotlight.
        let huge = r#"{"items":[{"label":"Huge","box_2d":[0,0,700,1000],"spoken":"H.","draw":"spotlight"}]}"#;
        assert_eq!(validate_script(huge).unwrap().items[0].draw, DrawStyle::Ring);
    }

    #[test]
    fn unknown_draw_falls_back_by_category() {
        for (cat, want) in [
            ("chart", DrawStyle::Bracket),
            ("diagram", DrawStyle::Bracket),
            ("code", DrawStyle::Highlight),
            ("text_block", DrawStyle::Highlight),
            ("ui_element", DrawStyle::Arrow),
            ("person", DrawStyle::Ring),
            ("food", DrawStyle::Ring),
            ("mystery", DrawStyle::Ring),
        ] {
            let raw = format!(
                "{{\"items\":[{{\"label\":\"X\",\"category\":\"{cat}\",\"box_2d\":[100,100,300,400],\"spoken\":\"X.\",\"draw\":\"sparkle\"}}]}}"
            );
            let s = validate_script(&raw).unwrap();
            assert_eq!(s.items[0].draw, want, "{cat}");
        }
    }

    #[test]
    fn intent_defaults_to_explain_and_survives_validation() {
        let none = r#"{"items":[{"label":"A","box_2d":[100,100,300,400],"spoken":"A."}]}"#;
        assert_eq!(validate_script(none).unwrap().intent, Intent::Explain);
        let summarise = r#"{"intent":"Summarise","items":[{"label":"A","box_2d":[100,100,300,400],"spoken":"A."}]}"#;
        assert_eq!(validate_script(summarise).unwrap().intent, Intent::Summarize);
        let junk = r#"{"intent":"nonsense","items":[{"label":"A","box_2d":[100,100,300,400],"spoken":"A."}]}"#;
        assert_eq!(validate_script(junk).unwrap().intent, Intent::Explain);
    }

    #[test]
    fn link_ids_survive_renumbering_when_an_earlier_item_is_dropped() {
        let raw = r#"{"items":[
          {"id":1,"label":"Dot","box_2d":[10,10,15,15],"spoken":"tiny"},
          {"id":2,"label":"A","box_2d":[100,100,300,400],"spoken":"First."},
          {"id":3,"label":"C","category":"text_block","box_2d":[500,500,700,800],"spoken":"Third.","draw":"connect","link_to":2}]}"#;
        let s = validate_script(raw).unwrap();
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.items[1].spatial.id, 2); // renumbered here; model id was 3
        assert_eq!(s.items[1].spatial.label, "C");
        assert_eq!(s.items[1].draw, DrawStyle::Connect);
        assert_eq!(s.items[1].link_to, Some(0));
    }

    #[test]
    fn prompt_asks_for_draw_style_and_intent() {
        let p = build_tour_prompt("explain my screen", "ctx");
        let lower = p.to_lowercase();
        for needle in ["spotlight", "lens", "connect", "intent", "link_to", "\"draw\""] {
            assert!(lower.contains(needle), "prompt missing {needle}");
        }
        // Existing budgets survive the extension.
        assert!(p.contains(&OVERVIEW_MAX_WORDS.to_string()));
        assert!(p.contains(&SPOKEN_MAX_WORDS.to_string()));
        assert!(p.contains(&CALLOUT_TITLE_MAX_WORDS.to_string()));
        assert!(p.contains(&CALLOUT_TEXT_MAX_WORDS.to_string()));
        assert!(p.contains(&ANSWER_MAX_WORDS.to_string()));
        assert!(p.contains(&MAX_ITEMS.to_string()));
    }

    #[test]
    fn callout_json_carries_draw_intent_link_and_lens_thumb() {
        let raw = r#"{"intent":"compare","items":[
          {"id":1,"label":"A","box_2d":[100,100,300,400],"spoken":"A.","draw":"connect","link_to":2},
          {"id":2,"label":"B","box_2d":[500,500,700,800],"spoken":"B."}]}"#;
        let s = validate_script(raw).unwrap();
        let j = callout_json("r1", &s, 0, (0, 0, 1920, 1080));
        assert_eq!(j["draw"], "connect");
        assert_eq!(j["intent"], "compare");
        assert_eq!(j["link"]["x"], 960); // 500/1000 * 1920
        assert_eq!(j["link"]["y"], 540);
        assert_eq!(j["link"]["width"], 576); // 300/1000 * 1920
        assert_eq!(j["link"]["height"], 216); // 200/1000 * 1080
        assert!(j["thumb"].is_null());
        // Existing fields unchanged.
        assert_eq!(j["total"], 2);
        assert_eq!(j["has_target"], true);

        let raw = r#"{"items":[
          {"label":"Fine print","box_2d":[100,100,200,250],"spoken":"Small.","draw":"lens"},
          {"label":"Other","box_2d":[400,400,600,600],"spoken":"O."}]}"#;
        let mut s = validate_script(raw).unwrap();
        s.items[0].thumb = Some("data:image/jpeg;base64,abc".to_string());
        s.items[1].thumb = Some("data:image/jpeg;base64,def".to_string());
        let lens = callout_json("r1", &s, 0, (0, 0, 1920, 1080));
        assert_eq!(lens["thumb"], "data:image/jpeg;base64,abc");
        assert_eq!(lens["link"], serde_json::Value::Null);
        let other = callout_json("r1", &s, 1, (0, 0, 1920, 1080));
        assert!(other["thumb"].is_null());
    }

    // ── Reverse-engineer mode ──

    #[test]
    fn stack_refs_are_trimmed_lowercased_deduped_and_capped() {
        let raw = r#"{"items":[{"label":"Hero","box_2d":[100,100,300,400],"spoken":"Hero.",
          "stack_refs":["NextJS","tailwind"," NEXTJS ","","react","vue","svelte"]}]}"#;
        let s = validate_script(raw).unwrap();
        assert_eq!(s.items[0].stack_refs, vec!["nextjs", "tailwind", "react", "vue"]);

        let plain = r#"{"items":[{"label":"B","box_2d":[100,100,300,400],"spoken":"B."}]}"#;
        assert!(validate_script(plain).unwrap().items[0].stack_refs.is_empty());
    }

    #[test]
    fn brand_is_kept_only_when_its_name_is_in_the_items_own_text() {
        let raw = r#"{"items":[
          {"label":"Gmail Trigger","box_2d":[100,100,300,400],"spoken":"Entry.","brand":"gmail.com"},
          {"label":"Slack Notification","box_2d":[500,100,700,400],"spoken":"Posts.","brand":"https://www.Slack.com/"},
          {"label":"FilterSender","box_2d":[100,500,300,800],"spoken":"Splits.","brand":"google.com"},
          {"label":"OpenRouter Chat Model","box_2d":[500,500,700,800],"spoken":"Model.","brand":"openrouter.ai"}]}"#;
        let s = validate_script(raw).unwrap();
        assert_eq!(s.items[0].brand.as_deref(), Some("gmail.com"));
        assert_eq!(s.items[1].brand.as_deref(), Some("slack.com"));
        assert_eq!(s.items[2].brand, None, "a brand whose name is not in the label is dropped");
        assert_eq!(s.items[3].brand.as_deref(), Some("openrouter.ai"));
    }

    #[test]
    fn brand_rejects_malformed_domains_and_missing_field() {
        let raw = r#"{"items":[
          {"label":"Gmail A","box_2d":[100,100,300,400],"spoken":"A.","brand":"gmail"},
          {"label":"Gmail B","box_2d":[500,100,700,400],"spoken":"B.","brand":"gm ail.com"},
          {"label":"Gmail C","box_2d":[100,500,300,800],"spoken":"C.","brand":"127.0.0.1"},
          {"label":"Gmail D","box_2d":[500,500,700,800],"spoken":"D.","brand":null},
          {"label":"Gmail E","box_2d":[100,900,300,990],"spoken":"E."}]}"#;
        let s = validate_script(raw).unwrap();
        for it in &s.items {
            assert_eq!(it.brand, None, "{}", it.spatial.label);
        }
    }

    #[test]
    fn brand_name_of_domain_handles_two_part_suffixes() {
        assert_eq!(brand_name_of_domain("gmail.com").as_deref(), Some("gmail"));
        assert_eq!(brand_name_of_domain("mail.google.co.uk").as_deref(), Some("google"));
        assert_eq!(brand_name_of_domain("a.io"), None);
        assert_eq!(brand_name_of_domain("localhost"), None);
    }

    #[test]
    fn prompt_asks_for_a_brand_field() {
        let p = build_tour_prompt("explain", "ctx");
        assert!(p.contains("\"brand\""));
    }

    #[test]
    fn validate_script_for_stack_keeps_known_ids_case_insensitively() {
        let raw = r#"{"items":[
          {"label":"Hero","box_2d":[100,100,300,400],"spoken":"Hero.",
           "stack_refs":["nextjs","tailwind","unknown","react"]},
          {"label":"Plain","box_2d":[500,500,700,800],"spoken":"Plain."}]}"#;
        let known = vec!["NextJS".to_string(), "TAILWIND".to_string(), "react".to_string()];
        let s = validate_script_for_stack(raw, &known).unwrap();
        assert_eq!(s.items[0].stack_refs, vec!["nextjs", "tailwind", "react"]);
        assert!(s.items[1].stack_refs.is_empty());
        // An empty known set drops every ref.
        let none = validate_script_for_stack(raw, &[]).unwrap();
        assert!(none.items[0].stack_refs.is_empty());
    }

    #[test]
    fn callout_json_carries_stack_refs() {
        let raw = r#"{"items":[{"label":"Hero","box_2d":[100,100,300,400],"spoken":"H.",
          "stack_refs":["nextjs","tailwind"]}]}"#;
        let s = validate_script(raw).unwrap();
        let j = callout_json("r1", &s, 0, (0, 0, 1000, 1000));
        assert_eq!(j["stack_refs"], serde_json::json!(["nextjs", "tailwind"]));

        let plain = r#"{"items":[{"label":"A","box_2d":[100,100,300,400],"spoken":"A."}]}"#;
        let p = validate_script(plain).unwrap();
        assert_eq!(
            callout_json("r1", &p, 0, (0, 0, 1000, 1000))["stack_refs"],
            serde_json::json!([])
        );
    }

    #[test]
    fn reverse_prompt_carries_sections_stack_and_contract() {
        let techs = vec![
            ("nextjs".to_string(), "Next.js".to_string()),
            ("tailwind".to_string(), "Tailwind CSS".to_string()),
        ];
        let prior = vec![
            "Hero: headline and call to action".to_string(),
            "Features grid".to_string(),
        ];
        let p = build_reverse_prompt(
            "reverse engineer this website",
            "ctx",
            "Next.js 15, Tailwind CSS, Vercel.",
            &techs,
            1,
            5,
            &prior,
        );
        for needle in [
            "REVERSE ENGINEER MODE",
            "section 2 of about 5",
            "nextjs",
            "Next.js",
            "tailwind",
            "Tailwind CSS",
            "Next.js 15, Tailwind CSS, Vercel.",
            "Hero: headline and call to action",
            "Features grid",
            "stack_refs",
            "box_2d",
            "at most 3",
        ] {
            assert!(p.contains(needle), "reverse prompt missing {needle}");
        }
        // The base rules and spoken budgets survive unchanged.
        assert!(p.contains("callout_text"));
        assert!(p.contains(&OVERVIEW_MAX_WORDS.to_string()));
        assert!(p.contains(&SPOKEN_MAX_WORDS.to_string()));
        assert!(p.contains(&CALLOUT_TITLE_MAX_WORDS.to_string()));
        assert!(p.contains(&ANSWER_MAX_WORDS.to_string()));
    }

    #[test]
    fn reverse_prompt_handles_empty_stack_and_history() {
        let p = build_reverse_prompt("how is this built", "", "", &[], 0, 1, &[]);
        assert!(p.contains("section 1 of about 1"));
        assert!(p.contains(
            "Detected technology stack (from the page's code, not from the image): none detected."
        ));
        assert!(p.contains("Known technology ids you may reference: none."));
        assert!(p.contains("Do not repeat sections already covered: none."));
    }
}
