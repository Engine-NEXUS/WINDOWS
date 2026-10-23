# Archive

Documents kept for history. They describe the earlier sidecar-era design (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite) or point-in-time snapshots, and **do not match the current code**. Each file starts with a "SUPERSEDED" banner.

For current behaviour read the repository `README.md`, [../README.md](../README.md) (docs index) and `AGENTS.md`.

| Archived | Why |
|---|---|
| `ARCHITECTURE.md`, `DEPLOYMENT.md`, `architecture/01–04` | Pre-serverless design (sidecar, n8n, Ollama, WebSocket protocol). Current design: Tauri/Rust client + Cloudflare Worker, see `docs/architecture/05–11`. |
| `credentials/01, 03, 07, 08` | Sidecar-era credential model (Fernet in SQLite, `/apikeys/*` on the sidecar). |
| `CHANGELOG-2026-08-30.md`, `CHANGELOG-2026-08-31.md`, `CONVERSATION_LOG.md`, `NEXUS_FEATURES_IMPLEMENTED.md` | Hackathon-era logs. Current history: `docs/changes/` and `AGENTS.md`. |
| `full-system-test-2026-09-08.md`, `test-results-2026-09-08.md`, `session-changes-2026-09-08.md` | Snapshots from a much smaller codebase (317 tests). |
| `cleanup-plan.md` | Repo-size plan written against Piper/sherpa resources that were later removed. |
| `MAHAYUDDH_UI_SPEC.md` | Unrelated web UI spec. |
