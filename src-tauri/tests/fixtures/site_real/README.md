# Stack-scanner accuracy fixtures

Each folder is one website as the scanner saw it, replayed offline by
`site_eval::tests::real_site_accuracy` (`src-tauri/src/site_eval.rs`).

- `page.json`: the exact output of one guarded fetch (`site_probe::fetch_raw`).
- `labels.json`: what must, may and must not be found (table ids), the bucket,
  the thin reasons the scan must report, and the source URL for every "must".
- `rendered.json` (optional): the hidden render's collector output for that page.
- `baseline.json` (this folder): per-category F1 the gate compares against.

The four folders ending in `.example` are **synthetic** (`"synthetic": true`):
hand-written to exercise the replay path, not captured from real sites. They
prove the harness works; they say nothing about real-site accuracy.

Adding real sites (owner-run, needs the network):

```
cargo run --example site_capture -- https://nextjs.org
```

Then fill `labels.json` by hand from the site's own statements, run the test,
and update `baseline.json` with the printed F1 values only when a change is an
intended improvement. Target set: 50 sites spread as in
`docs/research/capabilities-audit/03-reverse-engineering.md` §7.
