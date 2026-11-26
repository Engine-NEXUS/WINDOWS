//! site_eval — offline accuracy harness for the website stack scanner.
//!
//! Capture once, replay forever: `dev_tools/site_capture.rs` saves what one
//! guarded fetch of a public site returned (`page.json`, the exact input of
//! `site_probe::analyse_captured`). Each fixture folder also holds a hand-made
//! `labels.json` (what must, may and must not be found, with the URL of the
//! site's own statement) and optionally `rendered.json` (the hidden render's
//! collector output). The test `real_site_accuracy` replays every folder,
//! scores per category and fails on a regression. CI never touches the network.
//!
//! Public because the capture tool in `dev_tools/` uses it; its API only
//! exposes plain strings and numbers.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Categories whose F1 is gated (audit 03 §7).
pub const HEADLINE_CATEGORIES: &[&str] =
    &["meta-framework", "frontend-framework", "hosting", "cdn", "cms", "ecommerce", "analytics"];

/// What a fixture expects. Ids are fingerprint-table ids.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Labels {
    #[serde(default)]
    pub must: Vec<String>,
    #[serde(default)]
    pub may: Vec<String>,
    #[serde(default)]
    pub must_not: Vec<String>,
    /// "cms", "ssr", "spa", "static", "challenge", "login" (the audit's spread).
    #[serde(default)]
    pub bucket: String,
    /// Thin reasons the scan must report ("challenge", "login_wall", "spa_shell", "http_error").
    #[serde(default)]
    pub expect_thin: Vec<String>,
    /// id -> URL where the site states it uses that technology.
    #[serde(default)]
    pub sources: BTreeMap<String, String>,
    /// True for hand-made fixtures (not a real capture).
    #[serde(default)]
    pub synthetic: bool,
}

/// True/false positives and misses for one category.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub tp: u32,
    pub fp: u32,
    pub fn_: u32,
}

impl Counts {
    pub fn precision(&self) -> f64 {
        if self.tp + self.fp == 0 { 1.0 } else { self.tp as f64 / (self.tp + self.fp) as f64 }
    }
    pub fn recall(&self) -> f64 {
        if self.tp + self.fn_ == 0 { 1.0 } else { self.tp as f64 / (self.tp + self.fn_) as f64 }
    }
    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) }
    }
}

/// Results over a fixture set.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Scores {
    pub sites: usize,
    pub per_category: BTreeMap<String, Counts>,
    /// status ("detected"/"likely"/"inferred") -> (correct, total)
    pub calibration: BTreeMap<String, (u32, u32)>,
    /// "host: id" for every must-not-find that appeared.
    pub must_not_hits: Vec<String>,
    /// "host: reason" for every expected thin reason that was missing.
    pub thin_misses: Vec<String>,
    /// "host: id" for every detection that was neither "must" nor "may".
    pub false_positives: Vec<String>,
    /// "host: id" for every "must" that was not found.
    pub misses: Vec<String>,
    /// "host: id" for every DETECTED technology in a single-valued category (meta-framework, CMS, e-commerce) that
    /// contradicts a labelled technology of the same category (the closest thing to a wrong answer when the labels
    /// are deliberately minimal: a site has one CMS, not two).
    pub conflicts: Vec<String>,
    /// How many Detected answers sat in a single-valued category on a site that has a label in that category.
    pub conflict_checked: u32,
}

/// Categories where a site normally has exactly one answer, so a second answer is a contradiction.
pub const SINGLE_VALUED: &[&str] = &["meta-framework", "cms", "ecommerce"];

/// Score one site's detections `(id, category, status)` against its labels.
/// `category_of` gives the table category of a labelled id that was missed. Pure.
pub fn score_site(
    host: &str,
    labels: &Labels,
    detections: &[(String, String, String)],
    category_of: &dyn Fn(&str) -> String,
    scores: &mut Scores,
) {
    scores.sites += 1;
    let found: Vec<&str> = detections.iter().map(|(id, _, _)| id.as_str()).collect();
    for (id, cat, status) in detections {
        let c = scores.per_category.entry(cat.clone()).or_default();
        let correct = labels.must.contains(id) || labels.may.contains(id);
        if labels.must.contains(id) {
            c.tp += 1;
        } else if !labels.may.contains(id) {
            c.fp += 1;
            scores.false_positives.push(format!("{host}: {id}"));
        }
        let cal = scores.calibration.entry(status.clone()).or_default();
        cal.1 += 1;
        if correct {
            cal.0 += 1;
        }
        if labels.must_not.contains(id) {
            scores.must_not_hits.push(format!("{host}: {id}"));
        }
        if status == "detected" && SINGLE_VALUED.contains(&cat.as_str()) && labels.must.iter().any(|m| category_of(m) == *cat) {
            scores.conflict_checked += 1;
            if !labels.must.contains(id) && !labels.may.contains(id) {
                scores.conflicts.push(format!("{host}: {id}"));
            }
        }
    }
    for id in &labels.must {
        if !found.contains(&id.as_str()) {
            scores.per_category.entry(category_of(id)).or_default().fn_ += 1;
            scores.misses.push(format!("{host}: {id}"));
        }
    }
}

fn thin_key(t: &crate::site_probe::ThinReason) -> &'static str {
    use crate::site_probe::ThinReason as T;
    match t {
        T::Challenge(_) => "challenge",
        T::LoginWall => "login_wall",
        T::SpaShell => "spa_shell",
        T::HttpError(_) => "http_error",
    }
}

/// Replay every fixture folder under `dir` (those with `labels.json` and
/// `page.json`) through the same analysis the live scan uses.
pub fn evaluate_dir(dir: &Path) -> Result<Scores, String> {
    let mut scores = Scores::default();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for site in entries {
        let host = site.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
        let (Ok(labels_text), Ok(page_text)) = (
            std::fs::read_to_string(site.join("labels.json")),
            std::fs::read_to_string(site.join("page.json")),
        ) else {
            continue;
        };
        let labels: Labels = serde_json::from_str(&labels_text).map_err(|e| format!("{host}/labels.json: {e}"))?;
        let page: crate::site_probe::CapturedPage =
            serde_json::from_str(&page_text).map_err(|e| format!("{host}/page.json: {e}"))?;
        let mut report = crate::site_probe::analyse_captured(&page).map_err(|e| format!("{host}: {e:?}"))?;
        if let Ok(rendered) = std::fs::read_to_string(site.join("rendered.json")) {
            // Stored as the collector's object; wrap it the way WebView2 returns it.
            let raw = serde_json::to_string(&rendered).map_err(|e| e.to_string())?;
            let parsed = crate::site_render::parse_rendered(&raw).map_err(|e| format!("{host}/rendered.json: {e}"))?;
            report = crate::site_render::merge_rendered_signals(&report, &parsed);
        }
        let dets: Vec<(String, String, String)> = report
            .detections
            .iter()
            .map(|d| {
                let status = match d.status {
                    crate::site_probe::Status::Detected => "detected",
                    crate::site_probe::Status::Likely => "likely",
                    crate::site_probe::Status::Inferred => "inferred",
                };
                (d.id.clone(), d.category.clone(), status.to_string())
            })
            .collect();
        let thin: Vec<&str> = report.thin.iter().map(thin_key).collect();
        for want in &labels.expect_thin {
            if !thin.contains(&want.as_str()) {
                scores.thin_misses.push(format!("{host}: {want}"));
            }
        }
        score_site(&host, &labels, &dets, &|id| crate::site_probe::category_of(id), &mut scores);
    }
    Ok(scores)
}

/// Fetch one public site with the scanner's own guarded fetch and return the
/// `page.json` text. Network: run by the owner through `site_capture`, never by tests.
pub async fn capture(url: &str) -> Result<String, String> {
    let page = crate::site_probe::fetch_raw(url, crate::site_probe::ProbeOpts::default())
        .await
        .map_err(|e| e.spoken())?;
    serde_json::to_string_pretty(&page).map_err(|e| e.to_string())
}

/// Human-readable result of analysing a saved `page.json` (what the live scan
/// would report, static pass only): thin reasons, then every detection.
pub fn report_page_json(page_json: &str) -> Result<String, String> {
    let page: crate::site_probe::CapturedPage = serde_json::from_str(page_json).map_err(|e| e.to_string())?;
    let r = crate::site_probe::analyse_captured(&page).map_err(|e| format!("{e:?}"))?;
    let mut out = format!(
        "{} -> {} (HTTP {}), title {:?}, html {} B, {} scripts fetched, {} inline\n",
        page.initial_url,
        page.final_url,
        page.status,
        r.title,
        page.html.len(),
        page.scripts.len(),
        r.stats.inline_script_count
    );
    for t in &r.thin {
        out.push_str(&format!("  THIN: {}\n", t.spoken()));
    }
    if r.detections.is_empty() {
        out.push_str("  (no technologies detected)\n");
    }
    for d in &r.detections {
        let status = match d.status {
            crate::site_probe::Status::Detected => "Detected",
            crate::site_probe::Status::Likely => "Likely  ",
            crate::site_probe::Status::Inferred => "Inferred",
        };
        let v = d.version.as_deref().map(|v| format!(" {v}")).unwrap_or_default();
        out.push_str(&format!(
            "  {status} {:>3.0}%  {}{v} [{}]  <- {}\n",
            d.confidence * 100.0,
            d.name,
            d.category,
            d.evidence.join("; ")
        ));
    }
    Ok(out)
}

/// 95 % Wilson score interval for `k` successes in `n` trials (the repo's rule for any rate it reports).
pub fn wilson95(k: u32, n: u32) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let (k, n) = (k as f64, n as f64);
    let z = 1.959_963_984_540_054_f64;
    let p = k / n;
    let denom = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denom;
    let half = z * ((p * (1.0 - p) / n) + z * z / (4.0 * n * n)).sqrt() / denom;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

/// The numbers a reader needs from a replay: recall of every labelled "must" with its interval, headline-category
/// precision/recall/F1, hard errors (a must-not that appeared), thin-reason misses and the list of misses.
pub fn summary(scores: &Scores) -> String {
    let (mut tp, mut fnn) = (0u32, 0u32);
    for c in scores.per_category.values() {
        tp += c.tp;
        fnn += c.fn_;
    }
    let (lo, hi) = wilson95(tp, tp + fnn);
    let mut out = format!(
        "{} sites, {} labelled technologies: {} found, {} missed -> recall {:.1}% (95% CI {:.1}% to {:.1}%)\n",
        scores.sites,
        tp + fnn,
        tp,
        fnn,
        100.0 * tp as f64 / (tp + fnn).max(1) as f64,
        100.0 * lo,
        100.0 * hi
    );
    // Labels list only what a source states, so "found but not labelled" is NOT an error and precision cannot be
    // read from them: the per-category lines show recall only (found of labelled).
    let (mut htp, mut hfn) = (0u32, 0u32);
    for cat in HEADLINE_CATEGORIES {
        let c = scores.per_category.get(*cat).copied().unwrap_or_default();
        if c.tp + c.fn_ == 0 {
            continue;
        }
        htp += c.tp;
        hfn += c.fn_;
        out.push_str(&format!("  {cat:<19} found {:>3} of {:>3} labelled (recall {:.2})\n", c.tp, c.tp + c.fn_, c.recall()));
    }
    let (rlo, rhi) = wilson95(htp, htp + hfn);
    out.push_str(&format!(
        "headline categories together: recall {:.1}% (CI {:.1}-{:.1}) over {} labelled\n",
        100.0 * htp as f64 / (htp + hfn).max(1) as f64,
        100.0 * rlo,
        100.0 * rhi,
        htp + hfn
    ));
    out.push_str(&format!("must-not hits (hard errors): {}\n", scores.must_not_hits.len()));
    for m in &scores.must_not_hits {
        out.push_str(&format!("  ! {m}\n"));
    }
    out.push_str(&format!("expected thin reasons missing: {}\n", scores.thin_misses.len()));
    for m in &scores.thin_misses {
        out.push_str(&format!("  ? {m}\n"));
    }
    // Labels are minimal on purpose (only what the site's own sources state), so "found but not labelled" is not
    // an error. The wrong answers that CAN be told are contradictions inside a single-valued category.
    let ok = scores.conflict_checked.saturating_sub(scores.conflicts.len() as u32);
    let (clo, chi) = wilson95(ok, scores.conflict_checked);
    out.push_str(&format!(
        "contradictions in single-valued categories: {} of {} detected answers ({:.1}% consistent, CI {:.1}-{:.1})\n",
        scores.conflicts.len(),
        scores.conflict_checked,
        100.0 * ok as f64 / scores.conflict_checked.max(1) as f64,
        100.0 * clo,
        100.0 * chi
    ));
    for c in &scores.conflicts {
        out.push_str(&format!("  x {c}\n"));
    }
    out.push_str("misses:\n");
    for m in &scores.misses {
        out.push_str(&format!("  - {m}\n"));
    }
    out.push_str(&format!("detections neither must nor may: {}\n", scores.false_positives.len()));
    out
}

/// A `labels.json` template for a freshly captured site.
pub fn labels_template(url: &str) -> String {
    let t = serde_json::json!({
        "synthetic": false,
        "bucket": "",
        "must": [],
        "may": [],
        "must_not": [],
        "expect_thin": [],
        "sources": {},
        "_note": format!("Captured from {url}. Fill must/may/must_not with table ids; every 'must' needs a source URL where the site states it."),
    });
    serde_json::to_string_pretty(&t).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_interval_matches_known_values_and_stays_inside_0_1() {
        // 8 of 10: Wilson 95 % is about 49.0 % to 94.3 %
        let (lo, hi) = wilson95(8, 10);
        assert!((lo - 0.490).abs() < 0.005 && (hi - 0.943).abs() < 0.005, "{lo} {hi}");
        let (lo, hi) = wilson95(0, 0);
        assert_eq!((lo, hi), (0.0, 1.0));
        let (lo, hi) = wilson95(50, 50);
        assert!(lo > 0.92 && hi <= 1.0, "{lo} {hi}");
        let (lo, hi) = wilson95(0, 20);
        assert!(lo == 0.0 && hi < 0.17, "{lo} {hi}");
    }

    #[test]
    fn summary_states_recall_with_its_interval_and_lists_hard_errors() {
        let mut s = Scores::default();
        let labels = Labels { must: vec!["nextjs".into(), "vercel".into()], must_not: vec!["nuxt".into()], ..Labels::default() };
        let dets = vec![
            ("nextjs".to_string(), "meta-framework".to_string(), "detected".to_string()),
            ("nuxt".to_string(), "meta-framework".to_string(), "inferred".to_string()),
        ];
        score_site("a.test", &labels, &dets, &|_| "hosting".to_string(), &mut s);
        let text = summary(&s);
        assert!(text.contains("1 sites, 2 labelled technologies: 1 found, 1 missed"), "{text}");
        assert!(text.contains("95% CI"), "{text}");
        assert!(text.contains("! a.test: nuxt"), "a must-not hit is shown: {text}");
        assert!(text.contains("- a.test: vercel"), "a miss is shown: {text}");
    }

    fn fixtures_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/site_real")
    }

    #[test]
    fn score_arithmetic() {
        let labels = Labels {
            must: vec!["nextjs".into(), "vercel".into()],
            may: vec!["nodejs".into()],
            must_not: vec!["wordpress".into()],
            ..Default::default()
        };
        let dets = vec![
            ("nextjs".to_string(), "meta-framework".to_string(), "detected".to_string()),
            ("nodejs".to_string(), "backend-language".to_string(), "likely".to_string()),
            ("wordpress".to_string(), "cms".to_string(), "inferred".to_string()),
        ];
        let mut s = Scores::default();
        score_site("x", &labels, &dets, &|id| if id == "vercel" { "hosting".into() } else { "?".into() }, &mut s);
        assert_eq!(s.per_category["meta-framework"], Counts { tp: 1, fp: 0, fn_: 0 });
        assert_eq!(s.per_category["hosting"], Counts { tp: 0, fp: 0, fn_: 1 });
        assert_eq!(s.per_category["cms"], Counts { tp: 0, fp: 1, fn_: 0 });
        assert!(!s.per_category.contains_key("backend-language") || s.per_category["backend-language"].fp == 0, "may is never a false positive");
        assert_eq!(s.must_not_hits, vec!["x: wordpress".to_string()]);
        assert_eq!(s.calibration["detected"], (1, 1));
        assert_eq!(s.calibration["inferred"], (0, 1));
        let c = Counts { tp: 3, fp: 1, fn_: 1 };
        assert!((c.precision() - 0.75).abs() < 1e-9);
        assert!((c.recall() - 0.75).abs() < 1e-9);
        assert!((c.f1() - 0.75).abs() < 1e-9);
        assert_eq!(Counts::default().f1(), 1.0, "nothing expected, nothing found");
    }

    #[test]
    fn labels_template_parses_back() {
        let t: Labels = serde_json::from_str(&labels_template("https://example.com/")).unwrap();
        assert!(t.must.is_empty() && !t.synthetic);
    }

    /// The regression gate: replay every saved site and compare with the
    /// recorded baseline. Fails when a headline category's F1 drops by more
    /// than 0.02, when a must-not-find appears, or an expected thin reason is lost.
    #[test]
    fn real_site_accuracy() {
        let dir = fixtures_dir();
        let scores = evaluate_dir(&dir).expect("fixtures replay");
        assert!(scores.sites >= 3, "fixture set missing: {}", dir.display());
        for (cat, c) in &scores.per_category {
            println!("[site_eval] {cat}: tp={} fp={} fn={} f1={:.3}", c.tp, c.fp, c.fn_, c.f1());
        }
        println!("[site_eval] false positives: {:?}", scores.false_positives);
        println!("[site_eval] misses: {:?}", scores.misses);
        for (status, (ok, total)) in &scores.calibration {
            println!("[site_eval] calibration {status}: {ok}/{total}");
        }
        assert!(scores.must_not_hits.is_empty(), "must-not-find hits: {:?}", scores.must_not_hits);
        assert!(scores.thin_misses.is_empty(), "expected thin reasons missing: {:?}", scores.thin_misses);
        let baseline: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("baseline.json")).expect("baseline.json"),
        )
        .expect("baseline parses");
        for cat in HEADLINE_CATEGORIES {
            let Some(base) = baseline.get(*cat).and_then(|v| v.as_f64()) else { continue };
            let base = &base;
            let now = scores.per_category.get(*cat).map(|c| c.f1()).unwrap_or(1.0);
            assert!(now + 0.02 >= *base, "{cat}: F1 fell from {base:.3} to {now:.3}");
        }
    }
}
