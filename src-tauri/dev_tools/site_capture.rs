//! Save one public website as an accuracy fixture for the stack scanner.
//!
//!   cargo run --example site_capture -- https://nextjs.org [--out tests/fixtures/site_real]
//!
//! Makes ONE guarded fetch through the scanner itself (`site_probe::fetch_raw`:
//! public hosts only, DNS-pinned, no cookies, size and time caps) and writes
//! `<out>/<host>/page.json` plus a `labels.json` template when none exists.
//! Fill the labels by hand from the site's own statements (each "must" needs a
//! source URL), then run `cargo test --lib site_eval::tests::real_site_accuracy`.
//! Respect each site's terms and robots rules; capture each site once.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `--replay <dir>`: re-analyse saved page.json files offline (no network).
    if let Some(dir) = args.iter().position(|a| a == "--replay").and_then(|i| args.get(i + 1)) {
        let mut sites: Vec<_> = std::fs::read_dir(dir).expect("replay dir").filter_map(|e| e.ok()).map(|e| e.path()).collect();
        sites.sort();
        for site in sites {
            if let Ok(page) = std::fs::read_to_string(site.join("page.json")) {
                match nexus_lib::site_eval::report_page_json(&page) {
                    Ok(report) => println!("{report}"),
                    Err(e) => eprintln!("{}: {e}", site.display()),
                }
            }
        }
        return;
    }
    let Some(url) = args.iter().find(|a| a.starts_with("http")).cloned() else {
        eprintln!("usage: site_capture <https://site> [--out <dir>]");
        std::process::exit(2);
    };
    let out_dir = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| format!("{}/tests/fixtures/site_real", env!("CARGO_MANIFEST_DIR")));
    let host = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .unwrap_or("site")
        .trim_start_matches("www.")
        .replace(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'), "_");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let page = match rt.block_on(nexus_lib::site_eval::capture(&url)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("capture refused or failed: {e}");
            std::process::exit(1);
        }
    };
    match nexus_lib::site_eval::report_page_json(&page) {
        Ok(report) => println!("{report}"),
        Err(e) => eprintln!("analysis failed: {e}"),
    }
    let dir = std::path::Path::new(&out_dir).join(&host);
    std::fs::create_dir_all(&dir).expect("create fixture folder");
    std::fs::write(dir.join("page.json"), page).expect("write page.json");
    let labels = dir.join("labels.json");
    if !labels.exists() {
        std::fs::write(&labels, nexus_lib::site_eval::labels_template(&url)).expect("write labels.json");
    }
    println!("saved {}", dir.display());
}
