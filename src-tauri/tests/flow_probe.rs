//! F0 baseline probe for the multi-flow workflow plan (docs/features/193-multi-flow-command-workflows-plan.md).
//! Offline and read-only: runs the deterministic parser over `tests/fixtures/flows/corpus_v0.jsonl` and prints how
//! TODAY's code reads each phrase, whole and split on " and ". Executes nothing. Not a pass/fail gate; the
//! only assertion is that the corpus loads. Run: cargo test --test flow_probe -- --nocapture
use nexus_lib::intent_parser::parse_deterministic;

fn show(text: &str) -> String {
    match parse_deterministic(text) {
        Some(r) => {
            let dbg = format!("{:?}", r.intent);
            dbg.split(|c: char| c == ' ' || c == '{' || c == '(').next().unwrap_or("?").to_string()
        }
        None => "None".into(),
    }
}

#[test]
fn flow_baseline_today() {
    let raw = include_str!("fixtures/flows/corpus_v0.jsonl");
    let mut rows = 0;
    println!("id | kind | whole sentence | halves split on ' and ' (left | right)");
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(line).expect("corpus line is JSON");
        let phrase = v["phrase"].as_str().unwrap();
        let whole = show(phrase);
        let halves = match phrase.split_once(" and ") {
            Some((l, r)) => format!("{} | {}", show(l), show(r)),
            None => "(no ' and ')".into(),
        };
        println!("{} | {} | {} | {}", v["id"].as_str().unwrap(), v["kind"].as_str().unwrap(), whole, halves);
        rows += 1;
    }
    assert!(rows >= 10, "corpus too small: {rows}");
}
