//! reverse_engineer — runner and payload builders for "Nexus, reverse engineer this website / app".
//!
//! Scans a website (via `site_probe`) or desktop app (via `app_probe`), docks the
//! unified sidebar on the left with the technology stack breakdown, and tours
//! the visible sections of the UI using vision and narrated callouts.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

// ---------------------------------------------------------------------------
// Pure core types and builders
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Section {
    pub idx: usize,
    pub title: String,
    pub summary: String,
    pub stack_refs: Vec<String>,
}

/// Target classification for reverse engineering.
/// Not cfg(windows) so it is testable on all platforms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    Website(String),
    App(String),
    NeedUrl,
}

/// Classify the target based on the foreground app executable name, URL, and app path.
/// - url is Some and starts with http:// or https:// -> Website(url)
/// - otherwise, if is_browser_process(app_exe_name) -> NeedUrl (never App, even if app_path is Some)
/// - otherwise app_path Some -> App(path); None -> NeedUrl
pub(crate) fn classify_target(app_exe_name: &str, url: Option<&str>, app_path: Option<&str>) -> Target {
    if let Some(u) = url {
        if u.starts_with("http://") || u.starts_with("https://") {
            return Target::Website(u.to_string());
        }
    }
    if crate::browser_url::is_browser_process(app_exe_name) {
        Target::NeedUrl
    } else if let Some(p) = app_path {
        Target::App(p.to_string())
    } else {
        Target::NeedUrl
    }
}

static PENDING_STACK: Mutex<Option<serde_json::Value>> = Mutex::new(None);

pub(crate) fn set_pending_stack(v: &serde_json::Value) {
    *PENDING_STACK.lock().unwrap() = Some(v.clone());
}

#[tauri::command]
pub fn get_pending_stack() -> Result<Option<serde_json::Value>, String> {
    let mut pending = PENDING_STACK.lock().unwrap();
    Ok(pending.take())
}

/// `kind` is "site" or "app" (the Tech Stack view words its heading by it).
pub(crate) fn scanning_payload_kind(title: &str, url: &str, kind: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "title": title,
        "url": url,
        "final_url": url,
        "status_code": 0,
        "lang": "",
        "description": "",
        "scanning": true,
        "rendering": false,
        "thin": [],
        "detections": [],
        "groups": [],
        "security": {
            "hsts": false,
            "csp": false,
            "x_frame_options": false,
            "x_content_type_options": false,
            "referrer_policy": false,
            "permissions_policy": false,
        },
        "stats": {
            "html_bytes": 0,
            "script_count": 0,
            "inline_script_count": 0,
            "stylesheet_count": 0,
            "image_count": 0,
            "js_bytes_fetched": 0,
            "css_bytes_fetched": 0,
            "third_party_hosts": [],
        },
        "notes": [],
        "sections": [],
        "markdown": "",
        "summary": "",
    })
}

/// The scanned site's own domain (for its header logo), preferring the final URL.
fn site_logo_domain(report: &crate::site_probe::SiteReport) -> Option<String> {
    crate::brand_logo::domain_from_website(&report.final_url)
        .or_else(|| crate::brand_logo::domain_from_website(&report.url))
}

/// Every domain whose logo the Tech Stack view can show for this report.
fn logo_domains(report: &crate::site_probe::SiteReport) -> Vec<String> {
    let mut out: Vec<String> = report
        .detections
        .iter()
        .filter_map(|d| d.website.as_deref().and_then(crate::brand_logo::domain_from_website))
        .collect();
    if let Some(d) = site_logo_domain(report) {
        out.push(d);
    }
    out.sort();
    out.dedup();
    out
}

/// Small patch sent after the first paint once logos are known:
/// `{ "site_logo": <data url|null>, "logos": { "<detection id>": <data url> } }`.
/// A patch (not a second full payload) so it can never overwrite sections or
/// the summary that were emitted in the meantime. Pure over the logo memory.
/// Ids of the detections the sidebar has no bundled logo for (it then draws a category glyph). The bundled table is the
/// same JSON the sidebar resolves from, so this reports exactly what the owner will see.
pub(crate) fn ids_without_icon(ids: &[String]) -> Vec<String> {
    let bundled: serde_json::Value = serde_json::from_str(include_str!("../../frontend/src/sidebar/techIcons.json")).unwrap_or_default();
    let scraped: serde_json::Value = serde_json::from_str(include_str!("../../frontend/src/sidebar/techIconsScraped.json")).unwrap_or_default();
    ids.iter().filter(|id| bundled.get(id.as_str()).is_none() && scraped.get(id.as_str()).is_none()).cloned().collect()
}

pub(crate) fn logo_patch_json(report: &crate::site_probe::SiteReport) -> serde_json::Value {
    let mut logos = serde_json::Map::new();
    for d in &report.detections {
        if let Some(url) = d
            .website
            .as_deref()
            .and_then(crate::brand_logo::domain_from_website)
            .and_then(|dom| crate::brand_logo::memory_logo(&dom))
        {
            logos.insert(d.id.clone(), serde_json::Value::String(url));
        }
    }
    serde_json::json!({
        "site_logo": site_logo_domain(report).and_then(|dom| crate::brand_logo::memory_logo(&dom)),
        "logos": logos,
    })
}

pub(crate) fn stack_payload_for_site(
    report: &crate::site_probe::SiteReport,
    scanning: bool,
    sections: &[Section],
    summary: &str,
) -> serde_json::Value {
    let detections: Vec<serde_json::Value> = report
        .detections
        .iter()
        .map(|d| {
            let status = match d.status {
                crate::site_probe::Status::Detected => "detected",
                crate::site_probe::Status::Likely => "likely",
                crate::site_probe::Status::Inferred => "inferred",
            };
            serde_json::json!({
                "id": d.id,
                "name": d.name,
                "category": d.category,
                "confidence": d.confidence,
                "status": status,
                "evidence": d.evidence,
                "website": d.website,
                "logo": d.website.as_deref()
                    .and_then(crate::brand_logo::domain_from_website)
                    .and_then(|dom| crate::brand_logo::memory_logo(&dom)),
            })
        })
        .collect();

    let groups: Vec<serde_json::Value> = crate::site_probe::group_by_category(report)
        .into_iter()
        .map(|(label, dets)| {
            let category = dets.first().map(|d| d.category.clone()).unwrap_or_default();
            let ids: Vec<String> = dets.iter().map(|d| d.id.clone()).collect();
            serde_json::json!({
                "category": category,
                "label": label,
                "ids": ids,
            })
        })
        .collect();

    let mut markdown = crate::site_probe::report_markdown(&report.title, report);
    if !sections.is_empty() {
        markdown.push_str("\n## Page walkthrough\n\n");
        markdown.push_str(&sections_markdown(sections));
    }
    markdown.push_str(PRIVACY_FOOTNOTE);

    let sections_json: Vec<serde_json::Value> = sections
        .iter()
        .map(|s| {
            serde_json::json!({
                "idx": s.idx,
                "title": s.title,
                "summary": s.summary,
                "stack_refs": s.stack_refs,
            })
        })
        .collect();

    let thin: Vec<String> = report.thin.iter().map(|t| t.spoken()).collect();
    serde_json::json!({
        "kind": "site",
        "title": report.title,
        "url": report.url,
        "final_url": report.final_url,
        "status_code": report.status_code,
        "lang": report.lang,
        "description": report.description,
        "scanning": scanning,
        "rendering": false,
        "thin": thin,
        "site_logo": site_logo_domain(report).and_then(|dom| crate::brand_logo::memory_logo(&dom)),
        "detections": detections,
        "groups": groups,
        "security": {
            "hsts": report.security.hsts,
            "csp": report.security.csp,
            "x_frame_options": report.security.xfo,
            "x_content_type_options": report.security.xcto,
            "referrer_policy": report.security.referrer_policy,
            "permissions_policy": report.security.permissions_policy,
        },
        "stats": {
            "html_bytes": report.stats.html_bytes,
            "script_count": report.stats.script_count,
            "inline_script_count": report.stats.inline_script_count,
            "stylesheet_count": report.stats.stylesheet_count,
            "image_count": report.stats.image_count,
            "js_bytes_fetched": report.stats.js_bytes,
            "css_bytes_fetched": report.stats.css_bytes,
            "third_party_hosts": report.stats.third_party_hosts,
        },
        "notes": report.notes,
        "sections": sections_json,
        "markdown": markdown,
        "summary": summary,
    })
}

const APP_CATEGORY_ORDER: &[(&str, &str)] = &[
    ("app-framework", "Application framework"),
    ("runtime", "Runtime"),
    ("ui-toolkit", "UI toolkit"),
    ("packaging", "Packaging"),
    ("engine", "Engine"),
    ("browser-engine", "Browser engine"),
];

pub(crate) fn stack_payload_for_app(
    report: &crate::app_probe::AppReport,
    scanning: bool,
    sections: &[Section],
    summary: &str,
) -> serde_json::Value {
    let detections: Vec<serde_json::Value> = report
        .detections
        .iter()
        .map(|d| {
            let status = if d.confidence >= 0.8 {
                "detected"
            } else if d.confidence >= 0.5 {
                "likely"
            } else {
                "inferred"
            };
            serde_json::json!({
                "id": d.id,
                "name": d.name,
                "category": d.category,
                "confidence": d.confidence,
                "status": status,
                "evidence": d.evidence,
                "website": serde_json::Value::Null,
            })
        })
        .collect();

    let mut groups: Vec<serde_json::Value> = Vec::new();
    for (cat_slug, label) in APP_CATEGORY_ORDER {
        let ids: Vec<String> = report
            .detections
            .iter()
            .filter(|d| d.category == *cat_slug)
            .map(|d| d.id.clone())
            .collect();
        if !ids.is_empty() {
            groups.push(serde_json::json!({
                "category": cat_slug,
                "label": label,
                "ids": ids,
            }));
        }
    }
    for d in &report.detections {
        if !APP_CATEGORY_ORDER.iter().any(|(c, _)| *c == d.category)
            && !groups.iter().any(|g| g["category"] == d.category)
        {
            let ids: Vec<String> = report
                .detections
                .iter()
                .filter(|x| x.category == d.category)
                .map(|x| x.id.clone())
                .collect();
            let label = d.category.replace('_', " ").replace('-', " ");
            groups.push(serde_json::json!({
                "category": d.category,
                "label": label,
                "ids": ids,
            }));
        }
    }

    let mut markdown = crate::app_probe::report_markdown(report);
    if !sections.is_empty() {
        markdown.push_str("\n## App walkthrough\n\n");
        markdown.push_str(&sections_markdown(sections));
    }

    let sections_json: Vec<serde_json::Value> = sections
        .iter()
        .map(|s| {
            serde_json::json!({
                "idx": s.idx,
                "title": s.title,
                "summary": s.summary,
                "stack_refs": s.stack_refs,
            })
        })
        .collect();

    let title = if let Some(p) = &report.product {
        if !p.is_empty() {
            p.clone()
        } else {
            report.exe_name.clone()
        }
    } else {
        report.exe_name.clone()
    };

    // The exe file name only: the full path can carry the user's name, and the
    // Tech Stack view would otherwise try to read it as a web address.
    let exe_file = exe_file_name(&report.exe_path);
    serde_json::json!({
        "kind": "app",
        "title": title,
        "url": exe_file,
        "final_url": exe_file,
        "rendering": false,
        "thin": [],
        "status_code": 0,
        "lang": "",
        "description": report.company.clone().unwrap_or_default(),
        "scanning": scanning,
        "detections": detections,
        "groups": groups,
        "security": {
            "hsts": false,
            "csp": false,
            "x_frame_options": false,
            "x_content_type_options": false,
            "referrer_policy": false,
            "permissions_policy": false,
        },
        "stats": {
            "html_bytes": 0,
            "script_count": 0,
            "inline_script_count": 0,
            "stylesheet_count": 0,
            "image_count": 0,
            "js_bytes_fetched": 0,
            "css_bytes_fetched": 0,
            "third_party_hosts": [],
        },
        "notes": report.notes,
        "sections": sections_json,
        "markdown": markdown,
        "summary": summary,
    })
}

/// "Next.js 14.2" when a version was captured, else the plain name.
fn name_with_version(d: &crate::site_probe::Detection) -> String {
    match d.version.as_deref() {
        Some(v) if !v.is_empty() => format!("{} {v}", d.name),
        _ => d.name.clone(),
    }
}

/// One spoken sentence about a website's stack. The verb follows the
/// confidence of the lead technology, so a guess never sounds like a fact:
/// Detected "is built with", Likely "looks like", Inferred "my best guess is".
pub(crate) fn summary_sentence_site(report: &crate::site_probe::SiteReport) -> String {
    let why = report.thin.first().map(|t| format!(" {}", t.spoken())).unwrap_or_default();
    if report.detections.is_empty() {
        return format!("I couldn't identify the technologies from outside, sir.{why}");
    }

    let find_cat = |cats: &[&str]| {
        cats.iter()
            .find_map(|c| report.detections.iter().find(|d| d.category == *c))
    };
    let fw = find_cat(&["meta-framework", "frontend-framework", "cms", "ecommerce"]);
    let host = find_cat(&["hosting", "cdn"]);
    let style = find_cat(&["css-framework", "ui-library"]);

    let lead = fw.or(host).or(style).unwrap_or(&report.detections[0]);
    let opener = match lead.status {
        crate::site_probe::Status::Detected => "This is built with",
        crate::site_probe::Status::Likely => "This looks like",
        crate::site_probe::Status::Inferred => "I can't confirm it, but my best guess is",
    };
    let mut head = match (fw, host, style) {
        (Some(f), Some(h), Some(s)) => format!(
            "{opener} {} on {}, styled with {}.",
            name_with_version(f), h.name, s.name
        ),
        (Some(f), Some(h), None) => format!("{opener} {} on {}.", name_with_version(f), h.name),
        (Some(f), None, Some(s)) => format!("{opener} {}, styled with {}.", name_with_version(f), s.name),
        (Some(f), None, None) => format!("{opener} {}.", name_with_version(f)),
        (None, Some(h), Some(s)) => format!("{opener} a site hosted on {}, styled with {}.", h.name, s.name),
        (None, Some(h), None) => format!("{opener} a site hosted on {}.", h.name),
        (None, None, Some(s)) => format!("{opener} a site styled with {}.", s.name),
        (None, None, None) => format!("{opener} {}.", name_with_version(lead)),
    };
    if lead.status == crate::site_probe::Status::Detected && fw.is_none() {
        // "This is built with a site hosted on X" reads badly.
        head = head.replacen("This is built with a site", "This is a site", 1);
    }

    let total = report.detections.len();
    let confirmed = report.detections.iter().filter(|d| d.confidence >= 0.8).count();
    let tech_str = if total == 1 {
        "1 technology".to_string()
    } else {
        format!("{total} technologies")
    };
    let conf_str = if confirmed == 1 {
        "1 is confirmed".to_string()
    } else {
        format!("{confirmed} are confirmed")
    };
    format!("{head} I found {tech_str}; {conf_str}.{why}")
}

pub(crate) fn summary_sentence_app(report: &crate::app_probe::AppReport) -> String {
    if report.detections.is_empty() {
        return "I couldn't identify the technologies from outside, sir.".to_string();
    }

    let fw = report
        .detections
        .iter()
        .find(|d| d.category == "app-framework")
        .map(|d| d.name.as_str());
    let rt = report
        .detections
        .iter()
        .find(|d| d.category == "runtime")
        .map(|d| d.name.as_str());
    let ui = report
        .detections
        .iter()
        .find(|d| d.category == "ui-toolkit")
        .map(|d| d.name.as_str());

    let head = match (fw, rt, ui) {
        (Some(f), Some(r), _) => format!("This looks like {f}, running on {r}."),
        (Some(f), None, Some(u)) => format!("This looks like {f}, using {u}."),
        (Some(f), None, None) => format!("This looks like {f}."),
        (None, Some(r), Some(u)) => format!("This looks like {u}, running on {r}."),
        (None, Some(r), None) => format!("This looks like a {r} application."),
        (None, None, Some(u)) => format!("This looks like it uses {u}."),
        (None, None, None) => format!("This looks like it uses {} .", report.detections[0].name)
            .replace(" .", "."),
    };

    let total = report.detections.len();
    let confirmed = report.detections.iter().filter(|d| d.confidence >= 0.8).count();
    let tech_str = if total == 1 {
        "1 technology".to_string()
    } else {
        format!("{total} technologies")
    };
    let conf_str = if confirmed == 1 {
        "1 is confirmed".to_string()
    } else {
        format!("{confirmed} are confirmed")
    };
    format!("{head} I found {tech_str}; {conf_str}.")
}

pub(crate) fn tech_brief(
    detections: &[(String, String, f32)],
) -> (String, Vec<(String, String)>) {
    let mut sorted: Vec<&(String, String, f32)> = detections.iter().collect();
    sorted.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let brief_items: Vec<String> = sorted
        .iter()
        .take(12)
        .map(|item| {
            let label = if item.2 >= 0.8 {
                "confirmed"
            } else if item.2 >= 0.5 {
                "likely"
            } else {
                "possible"
            };
            format!("{} ({})", item.1, label)
        })
        .collect();
    let brief = brief_items.join(", ");

    let top_20: Vec<(String, String)> = sorted
        .iter()
        .take(20)
        .map(|item| (item.0.clone(), item.1.clone()))
        .collect();

    (brief, top_20)
}

pub(crate) fn progress_json(
    phase: &str,
    text: &str,
    step: Option<usize>,
    total: Option<usize>,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "phase": phase,
        "text": text,
    });
    if let Some(s) = step {
        obj["step"] = serde_json::json!(s);
    }
    if let Some(t) = total {
        obj["total"] = serde_json::json!(t);
    }
    obj
}

const OPENING_LINES: &[&str] = &[
    "Give me a minute, sir.",
    "Okay sir, give me a moment.",
    "On it, sir. This will take a minute.",
];

pub(crate) fn opening_line(seed: u8) -> &'static str {
    OPENING_LINES[(seed as usize) % OPENING_LINES.len()]
}

/// How the scan went, which decides the tour and the closing line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScanOutcome {
    /// Not allowed (private address, banking page, system app): no screenshots,
    /// no tour. Holds the spoken refusal.
    Refused(String),
    /// Allowed but could not be read (timeout, not HTML, unreadable exe): a
    /// visual-only tour may still run. Holds the spoken reason.
    Failed(String),
    /// Read, but the page was a bot check, a sign-in page, an empty shell, or
    /// nothing was recognised. Holds the spoken reasons (may be empty).
    Thin(Vec<String>),
    Ok,
}

pub(crate) fn site_outcome(
    res: &Result<crate::site_probe::SiteReport, crate::site_probe::ProbeRefusal>,
) -> ScanOutcome {
    use crate::site_probe::ProbeRefusal as P;
    match res {
        Err(r @ (P::BadUrl | P::NotHttp | P::PrivateAddress | P::Sensitive)) => ScanOutcome::Refused(r.spoken()),
        Err(r) => ScanOutcome::Failed(r.spoken()),
        Ok(report) if !report.thin.is_empty() => {
            ScanOutcome::Thin(report.thin.iter().map(|t| t.spoken()).collect())
        }
        Ok(report) if report.detections.is_empty() => ScanOutcome::Thin(Vec::new()),
        Ok(_) => ScanOutcome::Ok,
    }
}

pub(crate) fn app_outcome(
    res: &Result<crate::app_probe::AppReport, crate::app_probe::AppRefusal>,
) -> ScanOutcome {
    use crate::app_probe::AppRefusal as A;
    match res {
        Err(r @ (A::Sensitive | A::System)) => ScanOutcome::Refused(r.spoken().to_string()),
        Err(r) => ScanOutcome::Failed(r.spoken().to_string()),
        Ok(report) if report.detections.is_empty() => ScanOutcome::Thin(Vec::new()),
        Ok(_) => ScanOutcome::Ok,
    }
}

/// Refused targets are never captured or toured; everything else may be.
pub(crate) fn tour_allowed(o: &ScanOutcome) -> bool {
    !matches!(o, ScanOutcome::Refused(_))
}

/// The scan itself worked (the progress strip ends on "done", not "error").
pub(crate) fn outcome_success(o: &ScanOutcome) -> bool {
    matches!(o, ScanOutcome::Ok | ScanOutcome::Thin(_))
}

/// Spoken before a tour whose scan failed, so the user knows the cards are empty for a reason.
pub(crate) const VISUAL_ONLY_LINE: &str = "I couldn't read the code, so this walkthrough is visual only, sir.";

/// Spoken when the user moves the mouse or switches windows mid-tour.
pub(crate) const TOOK_OVER_LINE: &str = "You took over, so I stopped the tour, sir.";

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// The last line of a run, honest about how it went. None = say nothing more
/// (a refusal was already spoken, or nothing useful happened).
pub(crate) fn closing_line(outcome: &ScanOutcome, is_app: bool, sections: usize, techs: usize) -> Option<String> {
    let sec = plural(sections, "section", "sections");
    let tech = plural(techs, "technology", "technologies");
    if *outcome == ScanOutcome::Ok && techs == 0 {
        // Never say "found 0 technologies": nothing found is a thin result.
        return closing_line(&ScanOutcome::Thin(Vec::new()), is_app, sections, techs);
    }
    match outcome {
        ScanOutcome::Refused(_) => None,
        ScanOutcome::Failed(_) => (sections > 0).then(|| {
            format!("That's the visual walkthrough, sir. I covered {sec}, but I couldn't read the code behind it.")
        }),
        ScanOutcome::Thin(reasons) => {
            if is_app {
                return Some(if sections > 0 {
                    "Walkthrough complete, sir. I couldn't identify the app's technologies from its install folder.".to_string()
                } else {
                    "I couldn't identify the app's technologies from its install folder, sir.".to_string()
                });
            }
            let why = reasons.first().cloned().unwrap_or_else(|| {
                "I couldn't identify its technologies from outside.".to_string()
            });
            Some(match (sections > 0, techs > 0) {
                (true, true) => format!("Walkthrough complete, sir. I covered {sec} and found {tech}. {why}"),
                (true, false) => format!("Walkthrough complete, sir. I covered {sec}. {why}"),
                (false, true) => format!("I found {tech}, sir. {why}"),
                (false, false) => format!("{why} Sorry, sir."),
            })
        }
        ScanOutcome::Ok => Some(if is_app {
            if sections > 0 {
                format!("Walkthrough complete, sir. I looked at the window and found {tech}.")
            } else {
                format!("I found {tech}, sir, but I couldn't analyse the window itself.")
            }
        } else if sections > 0 {
            format!("Walkthrough complete, sir. I covered {sec} and found {tech}.")
        } else {
            format!("I found {tech}, sir, but I couldn't walk through the page.")
        }),
    }
}

/// The capture area with NEXUS's own sidebar cut out: keeps the wider strip
/// beside it. Hiding the sidebar from capture is unreliable under GDI, so the
/// area is cropped instead. Pure.
pub(crate) fn capture_region_excluding(
    region: crate::screen_context::Region,
    sidebar: Option<crate::screen_context::Region>,
) -> crate::screen_context::Region {
    let Some(sb) = sidebar else { return region };
    let (r0, r1) = (region.x, region.x + region.w);
    let (s0, s1) = (sb.x, sb.x + sb.w);
    let v_overlap = sb.y < region.y + region.h && sb.y + sb.h > region.y;
    if s1 <= r0 || s0 >= r1 || !v_overlap {
        return region;
    }
    let left_w = (s0 - r0).max(0);
    let right_w = (r1 - s1).max(0);
    let (x, w) = if right_w >= left_w { (s1.max(r0), right_w) } else { (r0, left_w) };
    if w < 200 {
        return region;
    }
    crate::screen_context::Region { x, y: region.y, w, h: region.h }
}

/// Await `fut`, but give up (None) within ~100 ms of `cancel` being set.
pub(crate) async fn cancellable<F: std::future::Future>(
    fut: F,
    cancel: &std::sync::atomic::AtomicBool,
) -> Option<F::Output> {
    tokio::pin!(fut);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        tokio::select! {
            out = &mut fut => return Some(out),
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }
    }
}

/// The exe file name of a path ("C:\...\slack.exe" -> "slack.exe").
pub(crate) fn exe_file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
}

fn sections_markdown(sections: &[Section]) -> String {
    let mut md = String::new();
    for s in sections {
        md.push_str(&format!("### {}. {}\n{}\n\n", s.idx + 1, s.title, s.summary));
        if !s.stack_refs.is_empty() {
            md.push_str(&format!("*Technologies used: {}*\n\n", s.stack_refs.join(", ")));
        }
    }
    md
}

fn sections_json(sections: &[Section]) -> Vec<serde_json::Value> {
    sections
        .iter()
        .map(|s| {
            serde_json::json!({
                "idx": s.idx,
                "title": s.title,
                "summary": s.summary,
                "stack_refs": s.stack_refs,
            })
        })
        .collect()
}

/// Said in the report so the user knows what left the machine.
const PRIVACY_FOOTNOTE: &str = "\n---\n*How this was made: one anonymous fetch of the public page (no cookies), \
an optional hidden render of the same public page in an InPrivate browser window, screenshots of the visible page \
sent to your Gemini key for the walkthrough, and the detected technologies' domains sent to Logo.dev for icons.*\n";

/// The sidebar payload when the scan was refused or failed: keeps the reason
/// and any walkthrough sections (they used to be dropped on this path).
pub(crate) fn stack_payload_failed(
    title: &str,
    url: &str,
    kind: &str,
    reason: &str,
    sections: &[Section],
) -> serde_json::Value {
    let mut p = scanning_payload_kind(title, url, kind);
    p["scanning"] = serde_json::json!(false);
    p["notes"] = serde_json::json!([reason]);
    p["summary"] = serde_json::json!(reason);
    p["sections"] = serde_json::json!(sections_json(sections));
    let mut md = format!("## {title}\n\n{reason}\n");
    if !sections.is_empty() {
        md.push_str(if kind == "app" { "\n## App walkthrough\n\n" } else { "\n## Page walkthrough\n\n" });
        md.push_str(&sections_markdown(sections));
    }
    p["markdown"] = serde_json::json!(md);
    p
}

pub(crate) fn final_progress_phase(cancelled: bool, success: bool) -> &'static str {
    if cancelled {
        "cancelled"
    } else if !success {
        "error"
    } else {
        "done"
    }
}

// ---------------------------------------------------------------------------
// The runner `run`
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
pub(crate) async fn run<R: Runtime>(
    app: AppHandle<R>,
    _prompt: String,
) -> Result<crate::orchestrator::ProcessResult, String> {
    let (rid, _) =
        crate::orchestrator::install_new_request(crate::orchestrator::Subsystem::LocalCommand);
    crate::orchestrator::speak_line(
        &app,
        "Reverse engineering needs Windows for now, sir.".to_string(),
        &rid,
    );
    crate::orchestrator::clear_active_request(&rid);
    Ok(crate::orchestrator::local_result(
        rid,
        crate::orchestrator::Subsystem::LocalCommand,
    ))
}

#[cfg(target_os = "windows")]
pub(crate) async fn run<R: Runtime>(
    app: AppHandle<R>,
    prompt: String,
) -> Result<crate::orchestrator::ProcessResult, String> {
    use crate::orchestrator::{
        clear_active_request, emit, end_tour_fetch, install_new_request, is_cancelled_pub,
        local_result, narrate_tour_script, speak_line, OrchestratorEvent, OrchestratorState,
        Subsystem,
    };
    use crate::reverse_scroll::{max_viewports, plan_page_rect_beside, StepDecision};
    use crate::screen_tour::{analyze_reverse_image, RevTarget};
    use crate::window_tile::{PlaceResult, RealTileEnv, TileEnv};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let (rid, cancel) = install_new_request(Subsystem::LocalCommand);
    let (sw, sh) = crate::screen::primary_monitor_size().unwrap_or((1920, 1080));
    let mut ctx = crate::screen_context::collect_target(sw, sh).await;

    if let Some(reason) = crate::screen_context::capture_block_reason(&ctx) {
        speak_line(&app, reason.to_string(), &rid);
        clear_active_request(&rid);
        return Ok(local_result(rid, Subsystem::LocalCommand));
    }

    // The window the command came from. It is never moved, resized or maximised: a website is scanned in a new
    // window of its own (see `tour_hwnd`), so there is nothing to restore afterwards.
    let orig_hwnd = unsafe { GetForegroundWindow() };

    // A browser with no URL yet: run the address-bar reading ladder.
    let app_exe_name = crate::browser_url::foreground_process_name().unwrap_or_default();
    if crate::browser_url::is_browser_process(&app_exe_name) && ctx.url.is_none() {
        if let Some(resolved) = crate::browser_url::resolve_foreground_url_for_command().await {
            ctx.url = Some(resolved);
        }
    }
    let app_exe_path =
        crate::browser_url::foreground_process_path().map(|p| p.to_string_lossy().to_string());
    let target = classify_target(&app_exe_name, ctx.url.as_deref(), app_exe_path.as_deref());

    println!(
        "[REVERSE] target: {}",
        match &target {
            Target::Website(u) => format!(
                "website {}",
                crate::browser_url::extract_host(u).unwrap_or(u.clone())
            ),
            Target::App(p) => format!("app {}", exe_file_name(p)),
            Target::NeedUrl => "need-url".to_string(),
        }
    );

    let (target_url, target_app_path) = match &target {
        Target::Website(u) => (Some(u.clone()), None),
        Target::App(p) => (None, Some(p.clone())),
        Target::NeedUrl => {
            speak_line(
                &app,
                "I can't read this tab's address, sir. Click the address bar once and ask again."
                    .to_string(),
                &rid,
            );
            clear_active_request(&rid);
            return Ok(local_result(rid, Subsystem::LocalCommand));
        }
    };

    // In a meeting: no window move, no speech, no tour, no docked sidebar
    // (it would be on the screen share). The result waits as a pending stack.
    let meeting = app.try_state::<Arc<crate::meeting_detect::MeetingState>>();
    let quiet = meeting.as_ref().map(|m| m.should_suppress_tts()).unwrap_or(false);
    let spoken = !quiet;

    let mut loading_shown = false;
    if spoken {
        let seed = crate::network::uuid_v4().as_bytes()[0];
        emit(
            &app,
            &OrchestratorEvent::State { state: OrchestratorState::Thinking, request_id: rid.clone() },
        );
        // The Ack event speaks the line (a speak_line Result would close the turn).
        emit(
            &app,
            &OrchestratorEvent::Ack { text: opening_line(seed).to_string(), request_id: rid.clone() },
        );
        emit(&app, &OrchestratorEvent::Loading { visible: true, request_id: rid.clone() });
        crate::center::direct_ui(&app, crate::center::UiDirective::Loading(true));
        loading_shown = true;
    }
    let _ = app.emit(
        "reverse:progress",
        progress_json("starting", "Preparing reverse engineering scan...", None, None),
    );

    // The window the scan and the tour act on: a new browser window beside the sidebar, or the original in place.
    let mut tour_hwnd: isize = orig_hwnd.0 as isize;
    let mut sections: Vec<Section> = Vec::new();
    let mut summary_sentence = String::new();
    let mut total_techs = 0usize;
    let mut outcome = ScanOutcome::Failed("The scan did not run.".to_string());
    let mut site_report_saved: Option<crate::site_probe::SiteReport> = None;
    let mut app_report_saved: Option<crate::app_probe::AppReport> = None;
    // Title/url/kind/reason for the sidebar when there is no report.
    let mut failed_view: Option<(String, String, &'static str, String)> = None;
    let mut stop_line: Option<&'static str> = None;

    'work: {
        if let Some(url) = target_url.clone() {
            let scan_handle = tokio::spawn({
                let u = url.clone();
                async move { crate::site_probe::probe(&u).await }
            });

            let scan_pay = scanning_payload_kind(&ctx.window_title, &url, "site");
            set_pending_stack(&scan_pay);
            // The sidebar goes first so the page window can be sized from its REAL edge (flush at any display scale).
            if !quiet {
                let _ = crate::commands::unified_show_sidebar(&app, "stack", Some("left")).await;
            }
            crate::commands::emit_logged(&app, "sidebar:show_stack", scan_pay);

            // Open the page in a NEW window of the same browser and put it beside the sidebar. The original window
            // stays exactly as it was (never resized, never made full screen). If anything fails the scan runs in
            // place and moves nothing.
            if spoken && crate::browser_url::is_browser_process(&app_exe_name) {
                if let (Some(path), Some(sb), Some(work_area)) =
                    (app_exe_path.clone(), sidebar_region(&app), RealTileEnv.work_area())
                {
                    match plan_page_rect_beside(work_area, sb.x + sb.w, 12) {
                        None => println!("[REVERSE] place: skipped (screen too small to share)"),
                        Some(rect) => {
                            match crate::scan_window::open_new_window(std::path::Path::new(&path), &app_exe_name, &url).await {
                                None => println!("[REVERSE] place: no new browser window appeared, scanning in place"),
                                Some(h) => {
                                    let res = RealTileEnv.place(h, rect);
                                    println!("[REVERSE] place: new window {:?}", res);
                                    if matches!(res, PlaceResult::Placed) {
                                        tour_hwnd = h;
                                        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
                                        let _ = crate::live::commands::window::focus_window(HWND(h));
                                        ctx = crate::screen_context::collect_target(sw, sh).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let Some(joined) = cancellable(scan_handle, &cancel).await else { break 'work };
            let mut scan_res = joined.unwrap_or_else(|e| {
                Err(crate::site_probe::ProbeRefusal::Network(format!("scan task failed: {e}")))
            });

            // Hidden render: the same public page in an invisible InPrivate
            // window, so client-rendered apps are seen after they hydrate.
            if let Ok(static_report) = scan_res.clone() {
                if quiet {
                    if let Ok(r) = scan_res.as_mut() {
                        r.notes.push("Deeper render skipped during a meeting.".into());
                    }
                } else if static_report.final_url.starts_with("https://") {
                    let mut pay = stack_payload_for_site(&static_report, false, &[], "");
                    pay["rendering"] = serde_json::json!(true);
                    set_pending_stack(&pay);
                    crate::commands::emit_logged(&app, "sidebar:show_stack", pay);
                    let _ = app.emit(
                        "reverse:progress",
                        progress_json("rendering", "Opening the page in a hidden window to look deeper...", None, None),
                    );
                    match crate::site_render::render(&app, &static_report.final_url, &cancel).await {
                        crate::site_render::RenderResult::Page(page) => {
                            println!(
                                "[REVERSE] render: {} globals, {} dom, {} markers, {} resources",
                                page.globals.len(),
                                page.dom.len(),
                                page.markers.len(),
                                page.resources.len()
                            );
                            scan_res = Ok(crate::site_render::merge_rendered_signals(&static_report, &page));
                        }
                        crate::site_render::RenderResult::LoginRedirect => {
                            let mut r = static_report;
                            if !r.thin.contains(&crate::site_probe::ThinReason::LoginWall) {
                                r.thin.push(crate::site_probe::ThinReason::LoginWall);
                            }
                            scan_res = Ok(r);
                        }
                        crate::site_render::RenderResult::Cancelled => break 'work,
                        crate::site_render::RenderResult::Failed(e) => {
                            println!("[REVERSE] render failed: {e}");
                            if let Ok(r) = scan_res.as_mut() {
                                r.notes.push("The deeper render did not finish, so this is the static scan only.".into());
                            }
                        }
                    }
                } else if let Ok(r) = scan_res.as_mut() {
                    r.notes.push("Deeper render skipped: the site is not served over https.".into());
                }
            }

            outcome = site_outcome(&scan_res);
            let (stack_brief, top_techs) = match &scan_res {
                Ok(report) => {
                    total_techs = report.detections.len();
                    let brief_data: Vec<(String, String, f32)> = report
                        .detections
                        .iter()
                        .map(|d| (d.id.clone(), d.name.clone(), d.confidence))
                        .collect();
                    let (brief, top_ids) = tech_brief(&brief_data);
                    summary_sentence = summary_sentence_site(report);
                    let payload = stack_payload_for_site(report, false, &[], &summary_sentence);
                    set_pending_stack(&payload);
                    crate::commands::emit_logged(&app, "sidebar:show_stack", payload);
                    {
                        let all: Vec<String> = report.detections.iter().map(|d| d.id.clone()).collect();
                        let none = ids_without_icon(&all);
                        println!("[REVERSE] icons: {}/{} bundled, no logo for: {}", all.len() - none.len(), all.len(), if none.is_empty() { "-".to_string() } else { none.join(", ") });
                    }
                    // Logos arrive after the first paint (cached or Logo.dev, bounded),
                    // as a patch so nothing emitted meanwhile is overwritten.
                    {
                        let app_logo = app.clone();
                        let report_logo = report.clone();
                        tokio::spawn(async move {
                            let domains = logo_domains(&report_logo);
                            if domains.is_empty() {
                                return;
                            }
                            let got = tokio::time::timeout(
                                std::time::Duration::from_secs(6),
                                crate::brand_logo::logos_for_domains(&domains),
                            )
                            .await
                            .unwrap_or_default();
                            if !got.is_empty() {
                                let _ = app_logo.emit("sidebar:stack_logos", logo_patch_json(&report_logo));
                            }
                        });
                    }
                    let _ = app.emit("reverse:progress", progress_json("summary", &summary_sentence, None, None));
                    if spoken {
                        speak_line(&app, summary_sentence.clone(), &rid);
                    }
                    (brief, top_ids)
                }
                Err(refusal) => {
                    let reason = refusal.spoken();
                    let failed = stack_payload_failed(&ctx.window_title, &url, "site", &reason, &[]);
                    set_pending_stack(&failed);
                    crate::commands::emit_logged(&app, "sidebar:show_stack", failed);
                    failed_view = Some((ctx.window_title.clone(), url.clone(), "site", reason.clone()));
                    if spoken {
                        speak_line(&app, reason, &rid);
                    }
                    ("unknown (the code could not be read)".to_string(), Vec::new())
                }
            };
            site_report_saved = scan_res.ok();

            if quiet || !tour_allowed(&outcome) {
                break 'work;
            }
            if matches!(outcome, ScanOutcome::Failed(_)) && spoken {
                speak_line(&app, VISUAL_ONLY_LINE.to_string(), &rid);
            }

            let total = max_viewports(&prompt);
            let window_region = if ctx.region.w > 0 && ctx.region.h > 0 {
                ctx.region
            } else {
                crate::screen_context::Region { x: 0, y: 0, w: sw, h: sh }
            };
            let region = capture_region_excluding(window_region, sidebar_region(&app));
            let hwnd_raw = tour_hwnd;
            let doc = tokio::task::spawn_blocking(move || doc_rect_of(hwnd_raw)).await.ok().flatten();
            let (mx, my, mw, mh) = crate::reverse_scroll::measure_region(doc, (region.x, region.y, region.w, region.h));
            let measure = capture_region_excluding(
                crate::screen_context::Region { x: mx, y: my, w: mw, h: mh },
                sidebar_region(&app),
            );
            let baseline_tick = crate::memcore::resume::last_input_tick();
            let mut our_send: Option<u32> = None;
            let mut last_view = false;

            for i in 0..total {
                if is_cancelled_pub(&cancel) {
                    break;
                }
                let Some(b64) = capture_frame(&app, &region).await else {
                    if i == 0 && spoken {
                        speak_line(&app, "I couldn't capture this page, sir.".to_string(), &rid);
                    }
                    break;
                };
                let prior_sections: Vec<String> = sections.iter().map(|s| s.title.clone()).collect();
                let context_block = ctx.prompt_block();
                let Some(analysed) = cancellable(
                    analyze_reverse_image(
                        &app,
                        &prompt,
                        &context_block,
                        &stack_brief,
                        &top_techs,
                        i,
                        total,
                        &prior_sections,
                        &b64,
                        (region.x, region.y, region.w, region.h),
                        RevTarget::Site,
                    ),
                    &cancel,
                )
                .await
                else {
                    break;
                };
                let script = match analysed {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("reverse_engineer: analyze failed on viewport {i}: {e}");
                        if i == 0 && spoken {
                            speak_line(&app, "I couldn't read this page, sir.".to_string(), &rid);
                        }
                        break;
                    }
                };
                let end_reason = narrate_tour_script(&app, &rid, script.clone(), &cancel, (sw, sh)).await;
                if end_reason == crate::screen_tour::EndReason::Cancelled {
                    break;
                }
                sections.push(section_from_script(i, &script));
                emit_stack(&app, site_report_saved.as_ref(), None, failed_view.as_ref(), &sections, &summary_sentence);
                let _ = app.emit(
                    "reverse:progress",
                    progress_json("touring", &format!("Touring section {} of {}", i + 1, total), Some(i + 1), Some(total)),
                );

                if last_view || i + 1 >= total {
                    break;
                }
                if user_has_taken_over(tour_hwnd, baseline_tick, our_send) {
                    stop_line = Some(TOOK_OVER_LINE);
                    break;
                }
                match scroll_one_viewport(&app, hwnd_raw, measure, &cancel, &mut our_send).await {
                    StepDecision::Moved => {}
                    StepDecision::LastView => last_view = true,
                    StepDecision::NoMove | StepDecision::Stalled | StepDecision::Retry => break,
                }
            }
        } else if let Some(exe_path) = target_app_path.clone() {
            let exe_file = exe_file_name(&exe_path);
            let scan_pay = scanning_payload_kind(&ctx.window_title, &exe_file, "app");
            set_pending_stack(&scan_pay);
            if !quiet {
                let _ = crate::commands::unified_show_sidebar(&app, "stack", Some("left")).await;
            }
            crate::commands::emit_logged(&app, "sidebar:show_stack", scan_pay);

            let probe_path = exe_path.clone();
            let probe = tokio::time::timeout(
                std::time::Duration::from_secs(APP_PROBE_TIMEOUT_SECS),
                tokio::task::spawn_blocking(move || crate::app_probe::probe(&probe_path)),
            );
            let Some(joined) = cancellable(probe, &cancel).await else { break 'work };
            let app_res = match joined {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => Err(crate::app_probe::AppRefusal::Unreadable(format!("scan task failed: {e}"))),
                Err(_) => Err(crate::app_probe::AppRefusal::Unreadable("scan timed out".into())),
            };
            outcome = app_outcome(&app_res);

            match app_res {
                Ok(report) => {
                    total_techs = report.detections.len();
                    let brief_data: Vec<(String, String, f32)> = report
                        .detections
                        .iter()
                        .map(|d| (d.id.clone(), d.name.clone(), d.confidence))
                        .collect();
                    let (stack_brief, top_techs) = tech_brief(&brief_data);
                    summary_sentence = summary_sentence_app(&report);
                    let payload = stack_payload_for_app(&report, false, &[], &summary_sentence);
                    set_pending_stack(&payload);
                    crate::commands::emit_logged(&app, "sidebar:show_stack", payload);
                    let _ = app.emit("reverse:progress", progress_json("summary", &summary_sentence, None, None));
                    if spoken {
                        speak_line(&app, summary_sentence.clone(), &rid);
                    }
                    app_report_saved = Some(report);

                    if !quiet {
                        let window_region = if ctx.region.w > 0 && ctx.region.h > 0 {
                            ctx.region
                        } else {
                            crate::screen_context::Region { x: 0, y: 0, w: sw, h: sh }
                        };
                        let region = capture_region_excluding(window_region, sidebar_region(&app));
                        let analysed = match capture_frame(&app, &region).await {
                            Some(b64) => {
                                let context_block = ctx.prompt_block();
                                let Some(r) = cancellable(
                                    analyze_reverse_image(
                                        &app,
                                        &prompt,
                                        &context_block,
                                        &stack_brief,
                                        &top_techs,
                                        0,
                                        1,
                                        &[],
                                        &b64,
                                        (region.x, region.y, region.w, region.h),
                                        RevTarget::App,
                                    ),
                                    &cancel,
                                )
                                .await
                                else {
                                    break 'work;
                                };
                                r
                            }
                            None => Err("capture failed".to_string()),
                        };
                        match analysed {
                            Ok(script) => {
                                let _ = narrate_tour_script(&app, &rid, script.clone(), &cancel, (sw, sh)).await;
                                sections.push(section_from_script(0, &script));
                            }
                            Err(e) => {
                                tracing::warn!("reverse_engineer: app window analysis failed: {e}");
                                if spoken {
                                    speak_line(&app, "I couldn't analyse the window itself, sir.".to_string(), &rid);
                                }
                            }
                        }
                    }
                }
                Err(refusal) => {
                    let reason = refusal.spoken().to_string();
                    let failed = stack_payload_failed(&ctx.window_title, &exe_file, "app", &reason, &[]);
                    set_pending_stack(&failed);
                    crate::commands::emit_logged(&app, "sidebar:show_stack", failed);
                    failed_view = Some((ctx.window_title.clone(), exe_file.clone(), "app", reason.clone()));
                    if spoken {
                        speak_line(&app, reason, &rid);
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Cleanup & finish (success, refusal, failure and cancel all end here)
    // -----------------------------------------------------------------------
    if loading_shown {
        end_tour_fetch(&app, &rid, true);
    }
    let was_cancelled = is_cancelled_pub(&cancel);

    emit_stack(
        &app,
        site_report_saved.as_ref(),
        app_report_saved.as_ref(),
        failed_view.as_ref(),
        &sections,
        &summary_sentence,
    );

    // Done: if the owner pushed the sidebar behind their windows to see the whole screen, bring it back in front.
    crate::commands::raise_sidebar(&app);

    let phase = final_progress_phase(was_cancelled, outcome_success(&outcome));
    let _ = app.emit(
        "reverse:progress",
        progress_json(phase, &format!("Reverse engineering {phase}"), None, None),
    );

    if spoken && !was_cancelled {
        let line = stop_line.map(str::to_string).or_else(|| {
            closing_line(&outcome, target_app_path.is_some(), sections.len(), total_techs)
        });
        if let Some(line) = line {
            speak_line(&app, line, &rid);
        }
    }

    emit(&app, &OrchestratorEvent::Done { request_id: rid.clone() });
    clear_active_request(&rid);

    Ok(local_result(rid, Subsystem::LocalCommand))
}

/// The app scanner reads a folder; it never needs longer than this.
#[cfg(target_os = "windows")]
const APP_PROBE_TIMEOUT_SECS: u64 = 8;

fn section_from_script(idx: usize, script: &crate::screen_tour::TourScript) -> Section {
    let mut stack_refs: Vec<String> = Vec::new();
    for item in &script.items {
        for r in &item.stack_refs {
            if !stack_refs.contains(r) {
                stack_refs.push(r.clone());
            }
        }
    }
    Section { idx, title: script.title.clone(), summary: script.overview.clone(), stack_refs }
}

/// Send the current sidebar payload: the site or app report when there is one,
/// otherwise the refusal/failure view with any sections gathered so far.
#[cfg(target_os = "windows")]
fn emit_stack<R: Runtime>(
    app: &AppHandle<R>,
    site: Option<&crate::site_probe::SiteReport>,
    app_report: Option<&crate::app_probe::AppReport>,
    failed: Option<&(String, String, &'static str, String)>,
    sections: &[Section],
    summary: &str,
) {
    let payload = if let Some(r) = site {
        stack_payload_for_site(r, false, sections, summary)
    } else if let Some(r) = app_report {
        stack_payload_for_app(r, false, sections, summary)
    } else if let Some((title, url, kind, reason)) = failed {
        stack_payload_failed(title, url, kind, reason, sections)
    } else {
        return;
    };
    set_pending_stack(&payload);
    crate::commands::emit_logged(app, "sidebar:show_stack", payload);
}

/// Where NEXUS's own sidebar is on screen (physical px), if it is showing.
#[cfg(target_os = "windows")]
fn sidebar_region<R: Runtime>(app: &AppHandle<R>) -> Option<crate::screen_context::Region> {
    let win = app.get_webview_window("sidebar")?;
    if !win.is_visible().unwrap_or(false) {
        return None;
    }
    let pos = win.outer_position().ok()?;
    let size = win.outer_size().ok()?;
    Some(crate::screen_context::Region {
        x: pos.x,
        y: pos.y,
        w: size.width as i32,
        h: size.height as i32,
    })
}

/// The user moved the mouse, typed, or switched away from the toured window.
#[cfg(target_os = "windows")]
fn user_has_taken_over(orig_hwnd: isize, baseline_tick: Option<u32>, our_send: Option<u32>) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let fg = unsafe { GetForegroundWindow() };
    if fg.0 as isize != orig_hwnd {
        return true;
    }
    match (crate::memcore::resume::last_input_tick(), baseline_tick) {
        (Some(last), Some(base)) => crate::reverse_scroll::user_took_over(last, base, our_send, OUR_INPUT_SLACK_MS),
        _ => false,
    }
}

/// Input this soon after our own PageDown is ours, not the user's.
#[cfg(target_os = "windows")]
const OUR_INPUT_SLACK_MS: u32 = 400;

/// UI Automation document element of a top-level window. Blocking.
#[cfg(target_os = "windows")]
fn doc_element(hwnd: isize) -> Option<uiautomation::UIElement> {
    use uiautomation::controls::ControlType;
    use uiautomation::types::Handle;
    use uiautomation::UIAutomation;
    let automation = UIAutomation::new().ok()?;
    let window = automation.element_from_handle(Handle::from(hwnd)).ok()?;
    automation
        .create_matcher()
        .from(window)
        .timeout(700)
        .control_type(ControlType::Document)
        .find_first()
        .ok()
}

/// The page area of a browser window (no tab strip, no address bar). Blocking.
#[cfg(target_os = "windows")]
fn doc_rect_of(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    let r = doc_element(hwnd)?.get_bounding_rectangle().ok()?;
    let (x, y) = (r.get_left(), r.get_top());
    Some((x, y, r.get_right() - x, r.get_bottom() - y))
}

/// The page's vertical scroll position in percent, when the browser reports it. Blocking.
#[cfg(target_os = "windows")]
fn doc_scroll_percent(hwnd: isize) -> Option<f64> {
    use uiautomation::patterns::UIScrollPattern;
    doc_element(hwnd)?
        .get_pattern::<UIScrollPattern>()
        .ok()?
        .get_vertical_scroll_percent()
        .ok()
        .filter(|v| *v >= 0.0)
}

/// Scroll the page one screen down through UI Automation: no mouse, no keys,
/// no focus change. False when the browser does not offer it. Blocking.
#[cfg(target_os = "windows")]
fn doc_scroll_page(hwnd: isize) -> bool {
    use uiautomation::patterns::UIScrollPattern;
    use uiautomation::types::ScrollAmount;
    let Some(el) = doc_element(hwnd) else { return false };
    let Ok(pattern) = el.get_pattern::<UIScrollPattern>() else { return false };
    if !pattern.is_vertically_scrollable().unwrap_or(false) {
        return false;
    }
    pattern.scroll(ScrollAmount::NoAmount, ScrollAmount::LargeIncrement).is_ok()
}

/// Move the page about one screen down and decide whether to tour the new view.
/// UI Automation first (touches nothing the user uses); PageDown through the
/// shortcut router (same-window check) when the browser does not offer it.
#[cfg(target_os = "windows")]
async fn scroll_one_viewport<R: Runtime>(
    app: &AppHandle<R>,
    hwnd: isize,
    measure: crate::screen_context::Region,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
    our_send: &mut Option<u32>,
) -> crate::reverse_scroll::StepDecision {
    use crate::reverse_scroll::{estimate_scroll_shift, gray_from_jpeg_b64, rows_to_px, StepDecision, ViewportStepper};
    let mut stepper = ViewportStepper::new(measure.h);
    let mut prev = capture_frame(app, &measure).await.and_then(|b| gray_from_jpeg_b64(&b));
    loop {
        if cancel.load(Ordering::Relaxed) {
            return StepDecision::Stalled;
        }
        let pct_before = tokio::task::spawn_blocking(move || doc_scroll_percent(hwnd)).await.ok().flatten();
        let via_uia = tokio::task::spawn_blocking(move || doc_scroll_page(hwnd)).await.unwrap_or(false);
        if !via_uia {
            let c = cancel.clone();
            let out = tokio::task::spawn_blocking(move || {
                crate::shortcuts::exec::do_action_cancelled(crate::shortcuts::map::Action::ScrollDown, None, &c)
            })
            .await;
            *our_send = crate::memcore::resume::last_input_tick();
            if !matches!(out, Ok(crate::shortcuts::router::Outcome::Sent { .. })) {
                return StepDecision::Stalled;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(450)).await;
        let pct_after = tokio::task::spawn_blocking(move || doc_scroll_percent(hwnd)).await.ok().flatten();
        let next = capture_frame(app, &measure).await.and_then(|b| gray_from_jpeg_b64(&b));
        let shift_px = match (&prev, &next) {
            (Some(p), Some(n)) => estimate_scroll_shift(p, n).map(|rows| rows_to_px(rows, n.h, measure.h)),
            _ => None,
        };
        let decision = stepper.observe(shift_px, pct_before, pct_after);
        if next.is_some() {
            prev = next;
        }
        if decision != StepDecision::Retry {
            return decision;
        }
    }
}

#[cfg(target_os = "windows")]
async fn capture_frame<R: Runtime>(
    app: &AppHandle<R>,
    region: &crate::screen_context::Region,
) -> Option<String> {
    let excluded = crate::stage::set_capture_excluded(app, true);
    if excluded {
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
    let (rx, ry, rw, rh) = (region.x, region.y, region.w, region.h);
    let cap = tokio::task::spawn_blocking(move || {
        crate::vision::capture_region_jpeg_base64(rx, ry, rw, rh, 1024)
    })
    .await
    .ok()
    .flatten();
    if excluded {
        crate::stage::set_capture_excluded(app, false);
    }
    cap
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn logo_test_report() -> crate::site_probe::SiteReport {
        crate::site_probe::SiteReport {
            url: "https://shop.example.org/a".into(),
            final_url: "https://www.example.org/a".into(),
            status_code: 200,
            title: "Shop".into(),
            description: String::new(),
            lang: "en".into(),
            detections: vec![
                crate::site_probe::Detection {
                    id: "nextjs".into(),
                    name: "Next.js".into(),
                    category: "meta-framework".into(),
                    website: Some("https://nextjs.org".into()),
                    confidence: 0.95,
                    status: crate::site_probe::Status::Detected,
                    evidence: vec!["_next".into()],
                    version: None,
                },
                crate::site_probe::Detection {
                    id: "no-site".into(),
                    name: "NoSite".into(),
                    category: "other".into(),
                    website: None,
                    confidence: 0.5,
                    status: crate::site_probe::Status::Likely,
                    evidence: vec![],
                    version: None,
                },
            ],
            security: crate::site_probe::SecurityHeaders {
                hsts: false, csp: false, xfo: false, xcto: false,
                referrer_policy: false, permissions_policy: false,
            },
            stats: crate::site_probe::PageStats {
                html_bytes: 1, js_bytes: 1, css_bytes: 1, script_count: 0,
                inline_script_count: 0, stylesheet_count: 0, image_count: 0,
                third_party_hosts: vec![],
            },
            notes: vec![],
            thin: Vec::new(),
            ctx: None,
        }
    }

    #[test]
    fn ids_without_icon_reports_only_what_the_sidebar_cannot_draw() {
        let got = ids_without_icon(&["react".into(), "csp".into(), "definitely-not-a-tech".into()]);
        assert!(!got.contains(&"react".to_string()));
        assert!(got.contains(&"definitely-not-a-tech".to_string()));
    }

    #[test]
    fn logo_domains_and_patch_follow_the_logo_memory() {
        let report = logo_test_report();
        assert_eq!(logo_domains(&report), vec!["example.org".to_string(), "nextjs.org".to_string()]);

        // nothing resolved yet -> an empty patch, no site logo
        let before = logo_patch_json(&report);
        assert!(before["logos"].as_object().unwrap().is_empty() || before["logos"]["nextjs"].is_string());

        crate::brand_logo::remember_for_test("nextjs.org", "data:image/png;base64,AAAA");
        crate::brand_logo::remember_for_test("example.org", "data:image/png;base64,BBBB");
        let patch = logo_patch_json(&report);
        assert_eq!(patch["logos"]["nextjs"], "data:image/png;base64,AAAA");
        assert!(patch["logos"].get("no-site").is_none());
        assert_eq!(patch["site_logo"], "data:image/png;base64,BBBB");

        // the full payload carries the same logos
        let full = stack_payload_for_site(&report, false, &[], "s");
        assert_eq!(full["site_logo"], "data:image/png;base64,BBBB");
        assert_eq!(full["detections"][0]["logo"], "data:image/png;base64,AAAA");
        assert!(full["detections"][1]["logo"].is_null());
    }

    #[test]
    fn site_payload_builder_keys_and_groups() {
        let detections = vec![
            crate::site_probe::Detection {
                id: "nextjs".into(),
                name: "Next.js".into(),
                category: "meta-framework".into(),
                website: Some("https://nextjs.org".into()),
                confidence: 0.95,
                status: crate::site_probe::Status::Detected,
                evidence: vec!["_next/static in script src".into()],
                version: None,
            },
            crate::site_probe::Detection {
                id: "react".into(),
                name: "React".into(),
                category: "frontend-framework".into(),
                website: Some("https://react.dev".into()),
                confidence: 0.90,
                status: crate::site_probe::Status::Detected,
                evidence: vec!["react in bundle".into()],
                version: None,
            },
            crate::site_probe::Detection {
                id: "tailwind".into(),
                name: "Tailwind CSS".into(),
                category: "css-framework".into(),
                website: Some("https://tailwindcss.com".into()),
                confidence: 0.85,
                status: crate::site_probe::Status::Detected,
                evidence: vec!["tailwind utility classes".into()],
                version: None,
            },
            crate::site_probe::Detection {
                id: "vercel".into(),
                name: "Vercel".into(),
                category: "hosting".into(),
                website: Some("https://vercel.com".into()),
                confidence: 0.95,
                status: crate::site_probe::Status::Detected,
                evidence: vec!["x-vercel-id header".into()],
                version: None,
            },
            crate::site_probe::Detection {
                id: "framer-motion".into(),
                name: "Framer Motion".into(),
                category: "animation".into(),
                website: None,
                confidence: 0.65,
                status: crate::site_probe::Status::Likely,
                evidence: vec!["framer motion in js".into()],
                version: None,
            },
            crate::site_probe::Detection {
                id: "google-analytics".into(),
                name: "Google Analytics".into(),
                category: "analytics".into(),
                website: None,
                confidence: 0.40,
                status: crate::site_probe::Status::Inferred,
                evidence: vec!["gtag.js pattern".into()],
                version: None,
            },
        ];

        let report = crate::site_probe::SiteReport {
            url: "https://example.com/shop".into(),
            final_url: "https://example.com/shop".into(),
            status_code: 200,
            title: "Example Store".into(),
            description: "An example e-commerce store".into(),
            lang: "en".into(),
            detections,
            security: crate::site_probe::SecurityHeaders {
                hsts: true,
                csp: true,
                xfo: true,
                xcto: false,
                referrer_policy: true,
                permissions_policy: false,
            },
            stats: crate::site_probe::PageStats {
                html_bytes: 45000,
                js_bytes: 120000,
                css_bytes: 25000,
                script_count: 5,
                inline_script_count: 2,
                stylesheet_count: 2,
                image_count: 8,
                third_party_hosts: vec!["cdn.example.net".into()],
            },
            notes: vec!["note 1".into()],
            thin: Vec::new(),
            ctx: None,
        };

        let sections = vec![Section {
            idx: 0,
            title: "Hero Section".into(),
            summary: "Big promotional banner".into(),
            stack_refs: vec!["nextjs".into(), "tailwind".into()],
        }];

        let json = stack_payload_for_site(&report, false, &sections, "A summary sentence");

        // Verify all StackPayload keys exist
        for key in [
            "title",
            "url",
            "final_url",
            "status_code",
            "lang",
            "description",
            "scanning",
            "detections",
            "groups",
            "security",
            "stats",
            "notes",
            "sections",
            "markdown",
            "summary",
        ] {
            assert!(
                json.get(key).is_some(),
                "Missing key in StackPayload: {key}"
            );
        }

        assert_eq!(json["scanning"], false);
        assert_eq!(json["title"], "Example Store");
        assert_eq!(json["security"]["x_frame_options"], true);
        assert_eq!(json["security"]["x_content_type_options"], false);
        assert_eq!(json["stats"]["js_bytes_fetched"], 120000);
        assert_eq!(json["stats"]["css_bytes_fetched"], 25000);

        // Verify ids in groups exist in detections
        let det_ids: Vec<String> = json["detections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(det_ids.len(), 6);

        let groups = json["groups"].as_array().unwrap();
        assert!(!groups.is_empty());
        for g in groups {
            let ids = g["ids"].as_array().unwrap();
            assert!(!ids.is_empty());
            for id in ids {
                assert!(
                    det_ids.contains(&id.as_str().unwrap().to_string()),
                    "Group ID {id} not found in detections"
                );
            }
        }

        // Markdown contains Page walkthrough when sections exist
        let md = json["markdown"].as_str().unwrap();
        assert!(
            md.contains("## Page walkthrough"),
            "Markdown should contain Page walkthrough"
        );
        assert!(md.contains("Hero Section"));

        // When sections is empty, no Page walkthrough in markdown
        let json_no_sections = stack_payload_for_site(&report, true, &[], "Scanning...");
        assert_eq!(json_no_sections["scanning"], true);
        let md_no_sec = json_no_sections["markdown"].as_str().unwrap();
        assert!(
            !md_no_sec.contains("## Page walkthrough"),
            "Markdown should NOT contain Page walkthrough when sections is empty"
        );
    }

    #[test]
    fn app_payload_builder_keys_and_groups() {
        let detections = vec![
            crate::app_probe::AppDetection {
                id: "electron".into(),
                name: "Electron".into(),
                category: "app-framework".into(),
                confidence: 0.95,
                evidence: vec!["resources/app.asar found".into()],
            },
            crate::app_probe::AppDetection {
                id: "nodejs".into(),
                name: "Node.js".into(),
                category: "runtime".into(),
                confidence: 0.85,
                evidence: vec!["node runtime found".into()],
            },
            crate::app_probe::AppDetection {
                id: "chromium".into(),
                name: "Chromium".into(),
                category: "browser-engine".into(),
                confidence: 0.80,
                evidence: vec!["icudtl.dat found".into()],
            },
        ];

        let report = crate::app_probe::AppReport {
            exe_name: "slack.exe".into(),
            exe_path: "C:\\Program Files\\Slack\\slack.exe".into(),
            install_dir: "C:\\Program Files\\Slack".into(),
            file_version: Some("4.35.126".into()),
            company: Some("Slack Technologies".into()),
            product: Some("Slack".into()),
            detections,
            notes: vec!["reads install folder only".into()],
        };

        let sections = vec![Section {
            idx: 0,
            title: "Channel View".into(),
            summary: "Message list".into(),
            stack_refs: vec!["electron".into()],
        }];

        let json = stack_payload_for_app(&report, false, &sections, "Slack analysis");

        for key in [
            "title",
            "url",
            "final_url",
            "status_code",
            "lang",
            "description",
            "scanning",
            "detections",
            "groups",
            "security",
            "stats",
            "notes",
            "sections",
            "markdown",
            "summary",
        ] {
            assert!(
                json.get(key).is_some(),
                "Missing key in app StackPayload: {key}"
            );
        }

        assert_eq!(json["title"], "Slack");
        assert_eq!(json["kind"], "app");
        assert_eq!(json["url"], "slack.exe", "only the file name leaves the app");
        assert_eq!(json["status_code"], 0);
        assert_eq!(json["security"]["hsts"], false);
        assert_eq!(json["stats"]["html_bytes"], 0);

        let det = &json["detections"][0];
        assert_eq!(det["status"], "detected");
        assert!(det["website"].is_null());

        let groups = json["groups"].as_array().unwrap();
        assert!(!groups.is_empty());
        assert_eq!(groups[0]["category"], "app-framework");
        assert_eq!(groups[0]["label"], "Application framework");

        let md = json["markdown"].as_str().unwrap();
        assert!(md.contains("## App walkthrough"));
        assert!(!md.contains("Page walkthrough"));
    }

    #[test]
    fn scanning_payload_shape() {
        let p = scanning_payload_kind("Scanning Target", "https://example.com", "site");
        assert_eq!(p["scanning"], true);
        assert_eq!(p["kind"], "site");
        assert_eq!(p["rendering"], false);
        assert_eq!(p["title"], "Scanning Target");
        assert_eq!(p["url"], "https://example.com");
        assert_eq!(p["detections"].as_array().unwrap().len(), 0);
        assert_eq!(p["groups"].as_array().unwrap().len(), 0);
        assert_eq!(p["sections"].as_array().unwrap().len(), 0);
        assert_eq!(p["notes"].as_array().unwrap().len(), 0);
        assert_eq!(p["stats"]["third_party_hosts"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn summary_sentence_site_naming_and_empty() {
        let empty_report = crate::site_probe::SiteReport {
            url: "https://example.com".into(),
            final_url: "https://example.com".into(),
            status_code: 200,
            title: "Empty".into(),
            description: "".into(),
            lang: "en".into(),
            detections: vec![],
            security: Default::default(),
            stats: Default::default(),
            notes: vec![],
            thin: Vec::new(),
            ctx: None,
        };
        assert_eq!(
            summary_sentence_site(&empty_report),
            "I couldn't identify the technologies from outside, sir."
        );

        let full_report = crate::site_probe::SiteReport {
            url: "https://example.com".into(),
            final_url: "https://example.com".into(),
            status_code: 200,
            title: "Test".into(),
            description: "".into(),
            lang: "en".into(),
            detections: vec![
                crate::site_probe::Detection {
                    id: "nextjs".into(),
                    name: "Next.js".into(),
                    category: "meta-framework".into(),
                    website: None,
                    confidence: 0.95,
                    status: crate::site_probe::Status::Detected,
                    evidence: vec![],
                    version: None,
                },
                crate::site_probe::Detection {
                    id: "vercel".into(),
                    name: "Vercel".into(),
                    category: "hosting".into(),
                    website: None,
                    confidence: 0.90,
                    status: crate::site_probe::Status::Detected,
                    evidence: vec![],
                    version: None,
                },
                crate::site_probe::Detection {
                    id: "tailwind".into(),
                    name: "Tailwind CSS".into(),
                    category: "css-framework".into(),
                    website: None,
                    confidence: 0.85,
                    status: crate::site_probe::Status::Detected,
                    evidence: vec![],
                    version: None,
                },
            ],
            security: Default::default(),
            stats: Default::default(),
            notes: vec![],
            thin: Vec::new(),
            ctx: None,
        };
        let s = summary_sentence_site(&full_report);
        assert!(s.contains("Next.js on Vercel, styled with Tailwind CSS"));
        assert!(s.contains("3 technologies; 3 are confirmed"));
    }

    #[test]
    fn summary_sentence_app_naming_and_empty() {
        let empty_report = crate::app_probe::AppReport {
            exe_name: "test.exe".into(),
            exe_path: "C:\\test.exe".into(),
            install_dir: "C:\\".into(),
            file_version: None,
            company: None,
            product: None,
            detections: vec![],
            notes: vec![],
        };
        assert_eq!(
            summary_sentence_app(&empty_report),
            "I couldn't identify the technologies from outside, sir."
        );

        let full_report = crate::app_probe::AppReport {
            exe_name: "test.exe".into(),
            exe_path: "C:\\test.exe".into(),
            install_dir: "C:\\".into(),
            file_version: None,
            company: None,
            product: None,
            detections: vec![
                crate::app_probe::AppDetection {
                    id: "electron".into(),
                    name: "Electron".into(),
                    category: "app-framework".into(),
                    confidence: 0.95,
                    evidence: vec![],
                },
                crate::app_probe::AppDetection {
                    id: "nodejs".into(),
                    name: "Node.js".into(),
                    category: "runtime".into(),
                    confidence: 0.85,
                    evidence: vec![],
                },
            ],
            notes: vec![],
        };
        let s = summary_sentence_app(&full_report);
        assert!(s.contains("Electron, running on Node.js"));
        assert!(s.contains("2 technologies; 2 are confirmed"));
    }

    #[test]
    fn tech_brief_ordering_labels_and_caps() {
        let mut detections = Vec::new();
        for i in 0..25 {
            detections.push((
                format!("id_{i}"),
                format!("Name_{i}"),
                0.20 + (i as f32) * 0.03, // 0.20 to 0.92
            ));
        }

        let (brief, top_ids) = tech_brief(&detections);

        // brief should cap at 12 items
        let brief_count = brief.split(',').count();
        assert_eq!(brief_count, 12);

        // top_ids should cap at 20 items
        assert_eq!(top_ids.len(), 20);

        // Ordering: highest confidence first (Name_24 has highest: 0.92)
        assert!(brief.starts_with("Name_24 (confirmed)"));
        assert_eq!(top_ids[0].0, "id_24");
        assert_eq!(top_ids[0].1, "Name_24");

        // Labels check
        let sample = vec![
            ("c".into(), "ConfirmedTech".into(), 0.85),
            ("l".into(), "LikelyTech".into(), 0.65),
            ("p".into(), "PossibleTech".into(), 0.35),
        ];
        let (sample_brief, _) = tech_brief(&sample);
        assert!(sample_brief.contains("ConfirmedTech (confirmed)"));
        assert!(sample_brief.contains("LikelyTech (likely)"));
        assert!(sample_brief.contains("PossibleTech (possible)"));
    }

    #[test]
    fn opening_line_covers_all_seeds() {
        for s in 0..=255u8 {
            let line = opening_line(s);
            assert!(OPENING_LINES.contains(&line));
        }
    }

    #[test]
    fn closing_line_pluralises() {
        assert_eq!(
            closing_line(&ScanOutcome::Ok, false, 1, 1).unwrap(),
            "Walkthrough complete, sir. I covered 1 section and found 1 technology."
        );
        assert_eq!(
            closing_line(&ScanOutcome::Ok, false, 3, 14).unwrap(),
            "Walkthrough complete, sir. I covered 3 sections and found 14 technologies."
        );
    }

    #[test]
    fn closing_line_is_honest_about_every_outcome() {
        // A refusal was already spoken: nothing more, and never "complete".
        assert_eq!(closing_line(&ScanOutcome::Refused("no".into()), false, 0, 0), None);
        // A failed scan with no tour says nothing; with a tour it says visual only.
        assert_eq!(closing_line(&ScanOutcome::Failed("timeout".into()), false, 0, 0), None);
        let f = closing_line(&ScanOutcome::Failed("timeout".into()), false, 2, 0).unwrap();
        assert!(f.contains("visual walkthrough") && f.contains("couldn't read the code"), "{f}");
        // Thin: the reason is spoken, never a bare "found 0 technologies".
        let t = closing_line(
            &ScanOutcome::Thin(vec!["The site showed me a Cloudflare bot check, so I could only see its hosting.".into()]),
            false,
            3,
            1,
        )
        .unwrap();
        assert!(t.contains("Cloudflare bot check"), "{t}");
        let t0 = closing_line(&ScanOutcome::Thin(vec![]), false, 0, 0).unwrap();
        assert!(t0.contains("couldn't identify"), "{t0}");
        // Ok without a walkthrough does not claim one.
        let o = closing_line(&ScanOutcome::Ok, false, 0, 5).unwrap();
        assert!(!o.contains("Walkthrough complete") && o.contains("5 technologies"), "{o}");
        // App wording never mentions sections of a page.
        let a = closing_line(&ScanOutcome::Ok, true, 1, 3).unwrap();
        assert!(a.contains("window") && !a.contains("section"), "{a}");
        for o in [ScanOutcome::Ok, ScanOutcome::Thin(vec![]), ScanOutcome::Failed("x".into())] {
            for secs in [0, 1, 4] {
                if let Some(line) = closing_line(&o, false, secs, 0) {
                    assert!(!line.contains("found 0 technologies"), "{o:?}/{secs}: {line}");
                }
            }
        }
    }

    #[test]
    fn refused_targets_are_never_toured() {
        assert!(!tour_allowed(&ScanOutcome::Refused("I do not scan private or banking sites, sir.".into())));
        assert!(tour_allowed(&ScanOutcome::Failed("The scan took too long, sir.".into())));
        assert!(tour_allowed(&ScanOutcome::Thin(vec![])));
        assert!(tour_allowed(&ScanOutcome::Ok));
        assert!(!outcome_success(&ScanOutcome::Failed("x".into())));
        assert!(!outcome_success(&ScanOutcome::Refused("x".into())));
        assert!(outcome_success(&ScanOutcome::Thin(vec![])));
    }

    #[test]
    fn site_outcome_maps_refusals_failures_and_thin_pages() {
        use crate::site_probe::ProbeRefusal as P;
        for r in [P::Sensitive, P::PrivateAddress, P::BadUrl, P::NotHttp] {
            assert!(matches!(site_outcome(&Err(r)), ScanOutcome::Refused(_)));
        }
        for r in [P::Timeout, P::NotHtml, P::TooManyRedirects, P::Network("x".into())] {
            assert!(matches!(site_outcome(&Err(r)), ScanOutcome::Failed(_)));
        }
        let mut thin = logo_test_report();
        thin.thin = vec![crate::site_probe::ThinReason::LoginWall];
        match site_outcome(&Ok(thin)) {
            ScanOutcome::Thin(r) => assert!(r[0].contains("sign-in")),
            o => panic!("{o:?}"),
        }
        let mut empty = logo_test_report();
        empty.detections.clear();
        assert_eq!(site_outcome(&Ok(empty)), ScanOutcome::Thin(vec![]));
        assert_eq!(site_outcome(&Ok(logo_test_report())), ScanOutcome::Ok);
    }

    #[test]
    fn app_outcome_refuses_system_and_sensitive_apps() {
        use crate::app_probe::AppRefusal as A;
        assert!(matches!(app_outcome(&Err(A::System)), ScanOutcome::Refused(_)));
        assert!(matches!(app_outcome(&Err(A::Sensitive)), ScanOutcome::Refused(_)));
        assert!(matches!(app_outcome(&Err(A::NotFound)), ScanOutcome::Failed(_)));
    }

    fn reg(x: i32, y: i32, w: i32, h: i32) -> crate::screen_context::Region {
        crate::screen_context::Region { x, y, w, h }
    }

    #[test]
    fn capture_region_crops_the_sidebar_out() {
        let screen = reg(0, 0, 1920, 1040);
        // sidebar docked left: keep the page on the right
        assert_eq!(capture_region_excluding(screen, Some(reg(0, 0, 540, 1040))), reg(540, 0, 1380, 1040));
        // sidebar docked right: keep the left
        assert_eq!(capture_region_excluding(screen, Some(reg(1380, 0, 540, 1040))), reg(0, 0, 1380, 1040));
        // no sidebar, or it does not overlap: unchanged
        assert_eq!(capture_region_excluding(screen, None), screen);
        let page = reg(560, 0, 1360, 1040);
        assert_eq!(capture_region_excluding(page, Some(reg(0, 0, 540, 1040))), page);
        // nothing sensible left beside it: keep the region rather than a sliver
        assert_eq!(capture_region_excluding(reg(0, 0, 600, 800), Some(reg(0, 0, 540, 800))), reg(0, 0, 600, 800));
    }

    #[tokio::test]
    async fn cancellable_gives_up_soon_after_cancel() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f2 = flag.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            f2.store(true, Ordering::Relaxed);
        });
        let start = std::time::Instant::now();
        let out = cancellable(tokio::time::sleep(std::time::Duration::from_secs(30)), &flag).await;
        assert!(out.is_none());
        assert!(start.elapsed() < std::time::Duration::from_millis(400), "{:?}", start.elapsed());
        let done = cancellable(async { 7 }, &std::sync::atomic::AtomicBool::new(false)).await;
        assert_eq!(done, Some(7));
    }

    #[test]
    fn failed_payload_keeps_the_reason_and_the_sections() {
        let sections = vec![Section { idx: 0, title: "Hero".into(), summary: "Big banner".into(), stack_refs: vec![] }];
        let p = stack_payload_failed("Shop", "https://shop.example.com/", "site", "The scan took too long, sir.", &sections);
        assert_eq!(p["scanning"], false);
        assert_eq!(p["kind"], "site");
        assert_eq!(p["summary"], "The scan took too long, sir.");
        assert_eq!(p["sections"].as_array().unwrap().len(), 1);
        assert!(p["markdown"].as_str().unwrap().contains("Hero"));
    }

    #[test]
    fn summary_wording_follows_confidence_and_explains_thin_results() {
        let mut r = logo_test_report();
        r.detections = vec![crate::site_probe::Detection {
            id: "nextjs".into(),
            name: "Next.js".into(),
            category: "meta-framework".into(),
            website: None,
            confidence: 0.35,
            status: crate::site_probe::Status::Inferred,
            evidence: vec![],
            version: None,
        }];
        let s = summary_sentence_site(&r);
        assert!(s.contains("best guess is Next.js"), "{s}");
        r.detections[0].confidence = 0.6;
        r.detections[0].status = crate::site_probe::Status::Likely;
        assert!(summary_sentence_site(&r).contains("This looks like Next.js"));
        r.detections[0].confidence = 0.95;
        r.detections[0].status = crate::site_probe::Status::Detected;
        r.detections[0].version = Some("14.2".into());
        assert!(summary_sentence_site(&r).contains("This is built with Next.js 14.2"));
        r.thin = vec![crate::site_probe::ThinReason::Challenge("Cloudflare".into())];
        assert!(summary_sentence_site(&r).contains("Cloudflare bot check"));
        r.detections.clear();
        let none = summary_sentence_site(&r);
        assert!(none.contains("couldn't identify") && none.contains("Cloudflare"), "{none}");
    }

    /// Wiring pin: the first live run (build a3d6200b246d9b4a) showed the intent
    /// parsed and routed, then dropped, because only the Ghost-session table in
    /// `orchestrator.rs` dispatched it. Both tables must call the runner.
    #[test]
    fn orchestrator_dispatches_reverse_engineer_in_normal_and_ghost_mode() {
        let src = include_str!("orchestrator.rs");
        let arm = r#"if intent == "reverse_engineer" => {"#;
        let ghost_start = src
            .find("// ─── Ghost session routing")
            .expect("ghost routing marker in orchestrator.rs");
        let (normal, ghost) = src.split_at(ghost_start);
        for (name, part) in [("normal", normal), ("ghost", ghost)] {
            let at = part.find(arm).unwrap_or_else(|| panic!("{name}-mode table has no reverse_engineer arm"));
            let body = &part[at..(at + 400).min(part.len())];
            assert!(body.contains("crate::reverse_engineer::run("), "{name}-mode arm must call the runner");
        }
    }

    #[test]
    fn exe_file_name_hides_the_folder() {
        assert_eq!(exe_file_name("C:\\Users\\someone\\AppData\\Local\\slack\\slack.exe"), "slack.exe");
        assert_eq!(exe_file_name("/usr/bin/code"), "code");
        assert_eq!(exe_file_name("plain.exe"), "plain.exe");
    }

    #[test]
    fn pending_stack_take_semantics() {
        let val = serde_json::json!({ "test": "val" });
        set_pending_stack(&val);
        assert_eq!(get_pending_stack().unwrap(), Some(val.clone()));
        // Second call should return None due to take() semantics
        assert_eq!(get_pending_stack().unwrap(), None);

        // set replaces
        let val2 = serde_json::json!({ "test": "val2" });
        set_pending_stack(&val2);
        assert_eq!(get_pending_stack().unwrap(), Some(val2));
    }

    #[test]
    fn cleanup_decision_functions() {
        assert_eq!(final_progress_phase(true, true), "cancelled");
        assert_eq!(final_progress_phase(true, false), "cancelled");
        assert_eq!(final_progress_phase(false, false), "error");
        assert_eq!(final_progress_phase(false, true), "done");
    }

    #[test]
    fn progress_json_keys() {
        let p1 = progress_json("starting", "Starting scan", None, None);
        assert_eq!(p1["phase"], "starting");
        assert_eq!(p1["text"], "Starting scan");
        assert!(p1.get("step").is_none());
        assert!(p1.get("total").is_none());

        let p2 = progress_json("touring", "Touring", Some(2), Some(5));
        assert_eq!(p2["phase"], "touring");
        assert_eq!(p2["text"], "Touring");
        assert_eq!(p2["step"], 2);
        assert_eq!(p2["total"], 5);
    }

    #[test]
    fn classify_target_website_with_http_url() {
        // Any exe with https:// -> Website
        assert_eq!(
            classify_target("notepad.exe", Some("https://example.com/"), Some("C:\\Windows\\notepad.exe")),
            Target::Website("https://example.com/".to_string())
        );
        assert_eq!(
            classify_target("brave.exe", Some("http://localhost:3000"), None),
            Target::Website("http://localhost:3000".to_string())
        );
        // URL takes precedence over browser detection
        assert_eq!(
            classify_target("brave.exe", Some("https://github.com/user/repo"), None),
            Target::Website("https://github.com/user/repo".to_string())
        );
    }

    #[test]
    fn classify_target_website_rejects_non_http() {
        // about:blank, chrome://, empty -> not Website
        assert_eq!(
            classify_target("brave.exe", Some("about:blank"), None),
            Target::NeedUrl
        );
        assert_eq!(
            classify_target("chrome.exe", Some("chrome://settings"), None),
            Target::NeedUrl
        );
        assert_eq!(
            classify_target("brave.exe", Some(""), None),
            Target::NeedUrl
        );
        assert_eq!(
            classify_target("brave.exe", None, None),
            Target::NeedUrl
        );
    }

    #[test]
    fn classify_target_browser_no_url_is_needurl() {
        // Browser with no URL -> NeedUrl (never App, even with path)
        assert_eq!(
            classify_target("brave.exe", None, Some("C:\\Program Files\\Brave\\brave.exe")),
            Target::NeedUrl
        );
        assert_eq!(
            classify_target("chrome.exe", None, Some("/usr/bin/chrome")),
            Target::NeedUrl
        );
        assert_eq!(
            classify_target("msedge.exe", None, Some("C:\\Edge\\msedge.exe")),
            Target::NeedUrl
        );
        assert_eq!(
            classify_target("firefox.exe", None, Some("/usr/bin/firefox")),
            Target::NeedUrl
        );
    }

    #[test]
    fn classify_target_non_browser_with_path_is_app() {
        // Non-browser with path -> App
        assert_eq!(
            classify_target("notepad.exe", None, Some("C:\\Windows\\notepad.exe")),
            Target::App("C:\\Windows\\notepad.exe".to_string())
        );
        assert_eq!(
            classify_target("code.exe", None, Some("/usr/bin/code")),
            Target::App("/usr/bin/code".to_string())
        );
    }

    #[test]
    fn classify_target_non_browser_without_path_is_needurl() {
        // Non-browser without path -> NeedUrl
        assert_eq!(
            classify_target("notepad.exe", None, None),
            Target::NeedUrl
        );
        assert_eq!(
            classify_target("unknown.exe", None, None),
            Target::NeedUrl
        );
    }
}
