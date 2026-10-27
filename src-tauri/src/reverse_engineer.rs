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

pub(crate) fn scanning_payload(title: &str, url: &str) -> serde_json::Value {
    serde_json::json!({
        "title": title,
        "url": url,
        "final_url": url,
        "status_code": 0,
        "lang": "",
        "description": "",
        "scanning": true,
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
        for s in sections {
            markdown.push_str(&format!("### {}. {}\n{}\n\n", s.idx + 1, s.title, s.summary));
            if !s.stack_refs.is_empty() {
                markdown.push_str(&format!("*Technologies used: {}*\n\n", s.stack_refs.join(", ")));
            }
        }
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

    serde_json::json!({
        "title": report.title,
        "url": report.url,
        "final_url": report.final_url,
        "status_code": report.status_code,
        "lang": report.lang,
        "description": report.description,
        "scanning": scanning,
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
        markdown.push_str("\n## Page walkthrough\n\n");
        for s in sections {
            markdown.push_str(&format!("### {}. {}\n{}\n\n", s.idx + 1, s.title, s.summary));
            if !s.stack_refs.is_empty() {
                markdown.push_str(&format!("*Technologies used: {}*\n\n", s.stack_refs.join(", ")));
            }
        }
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

    serde_json::json!({
        "title": title,
        "url": report.exe_path,
        "final_url": report.exe_path,
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

pub(crate) fn summary_sentence_site(report: &crate::site_probe::SiteReport) -> String {
    if report.detections.is_empty() {
        return "I couldn't identify the technologies from outside, sir.".to_string();
    }

    let fw = report
        .detections
        .iter()
        .find(|d| d.category == "meta-framework")
        .or_else(|| report.detections.iter().find(|d| d.category == "frontend-framework"))
        .map(|d| d.name.as_str());

    let host = report
        .detections
        .iter()
        .find(|d| d.category == "hosting")
        .or_else(|| report.detections.iter().find(|d| d.category == "cdn"))
        .map(|d| d.name.as_str());

    let style = report
        .detections
        .iter()
        .find(|d| d.category == "css-framework")
        .or_else(|| report.detections.iter().find(|d| d.category == "ui-library"))
        .map(|d| d.name.as_str());

    let head = match (fw, host, style) {
        (Some(f), Some(h), Some(s)) => {
            format!("This looks like {f} on {h}, styled with {s}.")
        }
        (Some(f), Some(h), None) => format!("This looks like {f} on {h}."),
        (Some(f), None, Some(s)) => format!("This looks like {f}, styled with {s}."),
        (Some(f), None, None) => format!("This looks like {f}."),
        (None, Some(h), Some(s)) => {
            format!("This looks like a site hosted on {h}, styled with {s}.")
        }
        (None, Some(h), None) => format!("This looks like a site hosted on {h}."),
        (None, None, Some(s)) => format!("This looks like a site styled with {s}."),
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

pub(crate) fn closing_line(sections: usize, techs: usize) -> String {
    let sec_str = if sections == 1 {
        "1 section".to_string()
    } else {
        format!("{sections} sections")
    };
    let tech_str = if techs == 1 {
        "1 technology".to_string()
    } else {
        format!("{techs} technologies")
    };
    format!("Walkthrough complete, sir. I covered {sec_str} and found {tech_str}.")
}

pub(crate) fn needs_restore(moved: bool, _cancelled: bool) -> bool {
    moved
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
        clear_active_request, emit, end_tour_fetch, hide_loading, install_new_request,
        is_cancelled_pub, local_result, narrate_tour_script, show_loading, speak_line,
        OrchestratorEvent, OrchestratorState, Subsystem,
    };
    use crate::reverse_scroll::{
        estimate_scroll_shift, gray_from_jpeg_b64, max_viewports, plan_page_rect, rows_to_px,
        ScrollController, ScrollDecision, SIDEBAR_W,
    };
    use crate::screen_tour::analyze_reverse_image;
    use crate::window_tile::{PlaceResult, RealTileEnv, TileEnv};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, IsZoomed, SetWindowPos, ShowWindow, SWP_NOACTIVATE,
        SWP_NOZORDER, SW_MAXIMIZE, SW_RESTORE,
    };

    let (rid, cancel) = install_new_request(Subsystem::LocalCommand);
    let (sw, sh) = crate::screen::primary_monitor_size().unwrap_or((1920, 1080));
    let mut ctx = crate::screen_context::collect_target(sw, sh).await;

    if let Some(reason) = crate::screen_context::capture_block_reason(&ctx) {
        speak_line(&app, reason.to_string(), &rid);
        clear_active_request(&rid);
        return Ok(local_result(rid, Subsystem::LocalCommand));
    }

    let (orig_hwnd, orig_rect, was_zoomed) = unsafe {
        let h = GetForegroundWindow();
        let is_z = if h.0 != 0 { IsZoomed(h).as_bool() } else { false };
        let mut r = RECT::default();
        if h.0 != 0 {
            let _ = GetWindowRect(h, &mut r);
        }
        (h, r, is_z)
    };

    // If the foreground process is a browser and we don't have a URL yet,
    // run the URL resolution ladder to try to get it.
    let app_exe_name = crate::browser_url::foreground_process_name().unwrap_or_default();
    if crate::browser_url::is_browser_process(&app_exe_name) && ctx.url.is_none() {
        if let Some(resolved) = crate::browser_url::resolve_foreground_url_for_command().await {
            ctx.url = Some(resolved);
        }
    }

    let app_exe_path = crate::browser_url::foreground_process_path().map(|p| p.to_string_lossy().to_string());

    let target = classify_target(
        &app_exe_name,
        ctx.url.as_deref(),
        app_exe_path.as_deref(),
    );

    println!("[REVERSE] target: {}",
        match &target {
            Target::Website(u) => format!("website {}", crate::browser_url::extract_host(u).unwrap_or(u.clone())),
            Target::App(p) => format!("app {}", std::path::Path::new(p).file_name().and_then(|s| s.to_str()).unwrap_or(p)),
            Target::NeedUrl => "need-url".to_string(),
        }
    );

    // Extract the URL and app path from the target for use in the branches
    let target_url = match &target {
        Target::Website(u) => Some(u.clone()),
        _ => ctx.url.clone(),
    };
    let target_app_path = match &target {
        Target::App(p) => Some(p.clone()),
        _ => app_exe_path,
    };

    let is_website = matches!(&target, Target::Website(_));

    match target {
        Target::NeedUrl => {
            speak_line(
                &app,
                "I can't read this tab's address, sir. Click the address bar once and ask again.".to_string(),
                &rid,
            );
            clear_active_request(&rid);
            return Ok(local_result(rid, Subsystem::LocalCommand));
        }
        Target::App(_) | Target::Website(_) => {
            // Continue to the branches below
        }
    }

    let meeting = app.try_state::<Arc<crate::meeting_detect::MeetingState>>();
    let quiet = meeting
        .as_ref()
        .map(|m| m.should_suppress_tts())
        .unwrap_or(false);
    let spoken = !quiet;

    if spoken {
        let seed = crate::network::uuid_v4().as_bytes()[0];
        let ack_text = opening_line(seed);
        emit(
            &app,
            &OrchestratorEvent::State {
                state: OrchestratorState::Thinking,
                request_id: rid.clone(),
            },
        );
        emit(
            &app,
            &OrchestratorEvent::Ack {
                text: ack_text.to_string(),
                request_id: rid.clone(),
            },
        );
        speak_line(&app, ack_text.to_string(), &rid);
        emit(
            &app,
            &OrchestratorEvent::Loading {
                visible: true,
                request_id: rid.clone(),
            },
        );
        show_loading(&app);
    }
    let _ = app.emit(
        "reverse:progress",
        progress_json("starting", "Preparing reverse engineering scan...", None, None),
    );

    let mut moved = false;
    let mut sections: Vec<Section> = Vec::new();
    let mut summary_sentence = String::new();
    let mut is_success = false;
    let mut total_techs = 0usize;
    let mut site_report_saved: Option<crate::site_probe::SiteReport> = None;
    let mut app_report_saved: Option<crate::app_probe::AppReport> = None;

    // `is_website` / `target_url` / `target_app_path` come from classify_target above.

    if is_website {
        let url = target_url.clone().unwrap_or_default();
        let scan_handle = tokio::spawn({
            let u = url.clone();
            async move { crate::site_probe::probe(&u).await }
        });

        if spoken {
            if let Some(work_area) = RealTileEnv.work_area() {
                if let Some(target_rect) = plan_page_rect(work_area, SIDEBAR_W) {
                    let place_res = RealTileEnv.place(orig_hwnd.0, target_rect);
                    match place_res {
                        PlaceResult::Placed => {
                            moved = true;
                            println!("[REVERSE] place: {:?}", place_res);
                        }
                        PlaceResult::SkippedElevated
                        | PlaceResult::SkippedGone
                        | PlaceResult::Failed(_)
                        | PlaceResult::Unsupported => {
                            moved = false;
                            println!("[REVERSE] place: {:?}", place_res);
                        }
                    }
                }
            }
        }

        let scan_pay = scanning_payload(&ctx.window_title, &url);
        set_pending_stack(&scan_pay);
        let _ = crate::commands::unified_show_sidebar(&app, "stack", Some("left")).await;
        crate::commands::emit_logged(&app, "sidebar:show_stack", scan_pay);

        if moved {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            ctx = crate::screen_context::collect_target(sw, sh).await;
        }

        let scan_res = match scan_handle.await {
            Ok(r) => r,
            Err(e) => Err(crate::site_probe::ProbeRefusal::Network(format!("scan task failed: {e}"))),
        };

        let (site_report_opt, stack_brief, top_techs) = match scan_res {
            Ok(report) => {
                total_techs = report.detections.len();
                let brief_data: Vec<(String, String, f32)> = report
                    .detections
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone(), d.confidence))
                    .collect();
                let (brief, top_ids) = tech_brief(&brief_data);
                let sentence = summary_sentence_site(&report);
                summary_sentence = sentence.clone();
                let payload = stack_payload_for_site(&report, false, &[], &summary_sentence);
                set_pending_stack(&payload);
                crate::commands::emit_logged(&app, "sidebar:show_stack", payload);
                // Logos arrive after the first paint (cached or Logo.dev, bounded):
                // never blocks the scan; sent as a patch so nothing is overwritten.
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
                            let _ = app_logo
                                .emit("sidebar:stack_logos", logo_patch_json(&report_logo));
                        }
                    });
                }
                let _ = app.emit(
                    "reverse:progress",
                    progress_json("summary", &summary_sentence, None, None),
                );
                if spoken {
                    speak_line(&app, summary_sentence.clone(), &rid);
                }
                (Some(report), brief, top_ids)
            }
            Err(refusal) => {
                let refusal_spoken = refusal.spoken();
                let mut failed = scanning_payload(&ctx.window_title, &url);
                failed["scanning"] = serde_json::json!(false);
                failed["notes"] = serde_json::json!([refusal_spoken]);
                failed["summary"] = serde_json::json!(refusal_spoken);
                set_pending_stack(&failed);
                crate::commands::emit_logged(&app, "sidebar:show_stack", failed);
                if spoken {
                    speak_line(&app, refusal_spoken.to_string(), &rid);
                }
                (
                    None,
                    "unknown (the scan was refused or failed)".to_string(),
                    Vec::new(),
                )
            }
        };

        site_report_saved = site_report_opt;

        if !quiet {
            let total = max_viewports(&prompt);
            let region = if ctx.region.w > 0 && ctx.region.h > 0 {
                ctx.region
            } else {
                crate::screen_context::Region {
                    x: 0,
                    y: 0,
                    w: sw,
                    h: sh,
                }
            };
            let mut controller = ScrollController::new(region.h);

            for i in 0..total {
                if is_cancelled_pub(&cancel) {
                    break;
                }

                let frame_b64 = capture_frame(&app, &region).await;
                let Some(b64) = frame_b64 else {
                    if i == 0 && spoken {
                        speak_line(&app, "I couldn't read this page, sir.".to_string(), &rid);
                    }
                    break;
                };

                let prior_sections: Vec<String> =
                    sections.iter().map(|s| s.title.clone()).collect();
                let context_block = ctx.prompt_block();
                let analysed = analyze_reverse_image(
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
                )
                .await;

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

                let end_reason =
                    narrate_tour_script(&app, &rid, script.clone(), &cancel, (sw, sh)).await;
                if end_reason == crate::screen_tour::EndReason::Cancelled {
                    break;
                }

                let mut stack_refs: Vec<String> = Vec::new();
                for item in &script.items {
                    for r in &item.stack_refs {
                        if !stack_refs.contains(r) {
                            stack_refs.push(r.clone());
                        }
                    }
                }

                sections.push(Section {
                    idx: i,
                    title: script.title.clone(),
                    summary: script.overview.clone(),
                    stack_refs,
                });

                if let Some(report) = &site_report_saved {
                    let payload =
                        stack_payload_for_site(report, false, &sections, &summary_sentence);
                    set_pending_stack(&payload);
                    crate::commands::emit_logged(&app, "sidebar:show_stack", payload);
                }

                let _ = app.emit(
                    "reverse:progress",
                    progress_json(
                        "touring",
                        &format!("Touring section {} of {}", i + 1, total),
                        Some(i + 1),
                        Some(total),
                    ),
                );

                if i + 1 < total {
                    let mid_x = region.x + region.w / 2;
                    let mid_y = region.y + region.h / 2;
                    let c = cancel.clone();
                    let _ = crate::live::commands::mouse::move_eased(mid_x, mid_y, 150, move || {
                        c.load(Ordering::Relaxed)
                    });

                    let mut prev_gray = gray_from_jpeg_b64(&b64);
                    let mut total_shift = 0i32;
                    let mut wheel_steps = 0;
                    let target_shift = (0.60 * region.h as f64) as i32;
                    let mut hit_end = false;

                    while total_shift < target_shift && wheel_steps < 12 {
                        if is_cancelled_pub(&cancel) {
                            break;
                        }
                        let notches = controller.notches();
                        wheel_steps += 1;
                        let _ = crate::live::commands::mouse::scroll(notches);
                        tokio::time::sleep(std::time::Duration::from_millis(450)).await;

                        let Some(probe_b64) = capture_frame(&app, &region).await else {
                            break;
                        };
                        let Some(next_gray) = gray_from_jpeg_b64(&probe_b64) else {
                            break;
                        };
                        let shift_rows = prev_gray
                            .as_ref()
                            .and_then(|pg| estimate_scroll_shift(pg, &next_gray));
                        let shift_px =
                            shift_rows.map(|r| rows_to_px(r, next_gray.h, region.h));
                        let decision = controller.observe(shift_px);

                        if let Some(px) = shift_px {
                            total_shift += px;
                            prev_gray = Some(next_gray);
                        }

                        if decision == ScrollDecision::AtEnd || decision == ScrollDecision::Stalled
                        {
                            hit_end = true;
                            break;
                        }
                    }

                    if hit_end {
                        break;
                    }
                }
            }
        }
        is_success = true;
    } else if let Some(exe_path) = target_app_path {
        let probe_path = exe_path.clone();
        let app_report_res = match tokio::task::spawn_blocking(move || crate::app_probe::probe(&probe_path)).await {
            Ok(r) => r,
            Err(e) => Err(crate::app_probe::AppRefusal::Unreadable(format!("scan task failed: {e}"))),
        };

        match app_report_res {
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
                let _ = crate::commands::unified_show_sidebar(&app, "stack", Some("left")).await;
                crate::commands::emit_logged(&app, "sidebar:show_stack", payload);
                let _ = app.emit(
                    "reverse:progress",
                    progress_json("summary", &summary_sentence, None, None),
                );
                if spoken {
                    speak_line(&app, summary_sentence.clone(), &rid);
                }

                if !quiet {
                    let region = if ctx.region.w > 0 && ctx.region.h > 0 {
                        ctx.region
                    } else {
                        crate::screen_context::Region {
                            x: 0,
                            y: 0,
                            w: sw,
                            h: sh,
                        }
                    };

                    if let Some(b64) = capture_frame(&app, &region).await {
                        let context_block = ctx.prompt_block();
                        let analysed = analyze_reverse_image(
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
                        )
                        .await;

                        if let Ok(script) = analysed {
                            let _ = narrate_tour_script(
                                &app,
                                &rid,
                                script.clone(),
                                &cancel,
                                (sw, sh),
                            )
                            .await;

                            let mut stack_refs: Vec<String> = Vec::new();
                            for item in &script.items {
                                for r in &item.stack_refs {
                                    if !stack_refs.contains(r) {
                                        stack_refs.push(r.clone());
                                    }
                                }
                            }
                            sections.push(Section {
                                idx: 0,
                                title: script.title.clone(),
                                summary: script.overview.clone(),
                                stack_refs,
                            });
                        }
                    }
                }
                app_report_saved = Some(report);
                is_success = true;
            }
            Err(refusal) => {
                let mut failed = scanning_payload(&ctx.window_title, &exe_path);
                failed["scanning"] = serde_json::json!(false);
                failed["notes"] = serde_json::json!([refusal.spoken()]);
                failed["summary"] = serde_json::json!(refusal.spoken());
                set_pending_stack(&failed);
                let _ = crate::commands::unified_show_sidebar(&app, "stack", Some("left")).await;
                crate::commands::emit_logged(&app, "sidebar:show_stack", failed);
                if spoken {
                    speak_line(&app, refusal.spoken().to_string(), &rid);
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Cleanup & finish (success, error, cancel all flow here)
    // -----------------------------------------------------------------------
    end_tour_fetch(&app, &rid, spoken);
    hide_loading(&app);

    let was_cancelled = is_cancelled_pub(&cancel);

    if needs_restore(moved, was_cancelled) && orig_hwnd.0 != 0 {
        unsafe {
            if was_zoomed {
                let _ = ShowWindow(orig_hwnd, SW_MAXIMIZE);
            } else {
                let _ = ShowWindow(orig_hwnd, SW_RESTORE);
                let _ = SetWindowPos(
                    orig_hwnd,
                    HWND(0),
                    orig_rect.left,
                    orig_rect.top,
                    orig_rect.right - orig_rect.left,
                    orig_rect.bottom - orig_rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
    }

    if let Some(report) = &site_report_saved {
        let final_payload = stack_payload_for_site(report, false, &sections, &summary_sentence);
        set_pending_stack(&final_payload);
        crate::commands::emit_logged(&app, "sidebar:show_stack", final_payload);
    } else if let Some(report) = &app_report_saved {
        let final_payload = stack_payload_for_app(report, false, &sections, &summary_sentence);
        set_pending_stack(&final_payload);
        crate::commands::emit_logged(&app, "sidebar:show_stack", final_payload);
    }

    let phase = final_progress_phase(was_cancelled, is_success);
    let _ = app.emit(
        "reverse:progress",
        progress_json(phase, &format!("Reverse engineering {phase}"), None, None),
    );

    if is_success && spoken && !was_cancelled {
        speak_line(&app, closing_line(sections.len(), total_techs), &rid);
    }

    emit(&app, &OrchestratorEvent::Done { request_id: rid.clone() });
    clear_active_request(&rid);

    Ok(local_result(rid, Subsystem::LocalCommand))
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
                },
                crate::site_probe::Detection {
                    id: "no-site".into(),
                    name: "NoSite".into(),
                    category: "other".into(),
                    website: None,
                    confidence: 0.5,
                    status: crate::site_probe::Status::Likely,
                    evidence: vec![],
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
        }
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
            },
            crate::site_probe::Detection {
                id: "react".into(),
                name: "React".into(),
                category: "frontend-framework".into(),
                website: Some("https://react.dev".into()),
                confidence: 0.90,
                status: crate::site_probe::Status::Detected,
                evidence: vec!["react in bundle".into()],
            },
            crate::site_probe::Detection {
                id: "tailwind".into(),
                name: "Tailwind CSS".into(),
                category: "css-framework".into(),
                website: Some("https://tailwindcss.com".into()),
                confidence: 0.85,
                status: crate::site_probe::Status::Detected,
                evidence: vec!["tailwind utility classes".into()],
            },
            crate::site_probe::Detection {
                id: "vercel".into(),
                name: "Vercel".into(),
                category: "hosting".into(),
                website: Some("https://vercel.com".into()),
                confidence: 0.95,
                status: crate::site_probe::Status::Detected,
                evidence: vec!["x-vercel-id header".into()],
            },
            crate::site_probe::Detection {
                id: "framer-motion".into(),
                name: "Framer Motion".into(),
                category: "animation".into(),
                website: None,
                confidence: 0.65,
                status: crate::site_probe::Status::Likely,
                evidence: vec!["framer motion in js".into()],
            },
            crate::site_probe::Detection {
                id: "google-analytics".into(),
                name: "Google Analytics".into(),
                category: "analytics".into(),
                website: None,
                confidence: 0.40,
                status: crate::site_probe::Status::Inferred,
                evidence: vec!["gtag.js pattern".into()],
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
        assert_eq!(json["url"], "C:\\Program Files\\Slack\\slack.exe");
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
        assert!(md.contains("## Page walkthrough"));
    }

    #[test]
    fn scanning_payload_shape() {
        let p = scanning_payload("Scanning Target", "https://example.com");
        assert_eq!(p["scanning"], true);
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
                },
                crate::site_probe::Detection {
                    id: "vercel".into(),
                    name: "Vercel".into(),
                    category: "hosting".into(),
                    website: None,
                    confidence: 0.90,
                    status: crate::site_probe::Status::Detected,
                    evidence: vec![],
                },
                crate::site_probe::Detection {
                    id: "tailwind".into(),
                    name: "Tailwind CSS".into(),
                    category: "css-framework".into(),
                    website: None,
                    confidence: 0.85,
                    status: crate::site_probe::Status::Detected,
                    evidence: vec![],
                },
            ],
            security: Default::default(),
            stats: Default::default(),
            notes: vec![],
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
            closing_line(1, 1),
            "Walkthrough complete, sir. I covered 1 section and found 1 technology."
        );
        assert_eq!(
            closing_line(3, 14),
            "Walkthrough complete, sir. I covered 3 sections and found 14 technologies."
        );
        assert_eq!(
            closing_line(0, 0),
            "Walkthrough complete, sir. I covered 0 sections and found 0 technologies."
        );
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
        assert_eq!(needs_restore(true, false), true);
        assert_eq!(needs_restore(true, true), true);
        assert_eq!(needs_restore(false, false), false);
        assert_eq!(needs_restore(false, true), false);

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
