# 135 — Action Kernel: voice file control with quarantine delete and undo, provenance, Hub page, safe coding-agent spawning (2026-10-17)

Executes docs/features/140 (P0-P6). Decisions D1-D5 were not answered one by one; the owner said to start
executing, so the **recommended defaults from doc 140 were used and are recorded here as assumptions**:
D1 Smart mode, D2 Downloads / Desktop / Documents (+ folders added in the Hub), D3 spoken read-back + sidebar
preview, D4 no permanent delete from voice, D5 shell work stays delegated to Claude Code / Codex.

## What was built

| Phase | What | Where |
|---|---|---|
| P0 | Every side-effecting turn writes one audit line (`risk_of_intent` classifies all existing runners); file tools write richer lines; 90-day prune at boot | `action_kernel/mod.rs`, `audit.rs`, hook after the ownership gate in `orchestrator.rs` |
| P1 | `Risk`, `Origin`, `Tool`, `Receipt`; canonical-path module (OS-resolved `..`, junctions, symlinks, 8.3 names, UNC refused, device names / trailing dots / ADS refused); policy engine (forbidden -> roots -> provenance -> caps -> deny/ask/allow rules -> mode) | `paths.rs`, `policy.rs` |
| P2 | File tools with no shell: list / find (Read); create folder, copy, move, rename (Reversible, never overwrite, "keep both" naming, cross-drive move = copy + verify + trash the original); delete = move to `<root>\.nexus-trash\<batch>` with a manifest; undo journal; restore; age purge; TOCTOU re-check + verify | `files.rs`, `journal.rs`, `resolve.rs` |
| P3 | Voice grammar (`file_action`, `file_undo` intents, `FileCenter`), object resolution to real entries, ambiguity asks "which one", "delete everything / wipe my drive" claimed and refused locally, read-back + sidebar card + `Offer::FileConfirm`, strict yes, owner-voice requirement | `voice.rs`, `pending.rs`, `orchestrator.rs`, `intent_parser.rs`, `center.rs`, `memcore/offer.rs` |
| P4 | Provenance: only a captured-voice turn (turn context present) is `UserVoice`; webhooks / queued follow-ups are `Derived` and can never delete or answer the confirmation; a file action can never run inside a compound command | `Origin::from_turn`, `command_center::build_plan`, offer gate |
| P5 | Hub page "Actions & Permissions": master switch, mode, folders, rules, coding-agent approval, trash list with Restore, Undo, recent activity. Policy edits apply at once (the file is re-read per command). IPC: `actions_get_policy`, `actions_set_policy` (sanitised), `actions_audit`, `actions_trash_list`, `actions_restore`, `actions_undo` | `action_kernel/ipc.rs`, `frontend/.../hub/ActionsPage.tsx`, `actionsModel.ts` |
| P6 | Coding agents started by voice: see "Real hole found" below | `agent_watch.rs`, `agent_watch_sidecar/server.cjs`, `coder/agents.rs` |

Never touched by any setting (fixed in code): drive roots, Windows / Program Files / ProgramData, other users'
profiles and the profiles folder, the user's whole profile folder, `AppData`, NEXUS's own data directory,
anything named `.git` / `.ssh` / `.gnupg` / `.aws` / `.nexus-trash`, the work folders themselves (only their
contents), shortcut links / junctions. Default rules also refuse `*.kdbx *.pem *.key *.pfx *.p12 id_rsa* id_ed25519* .env *.env *.ppk`
for delete / move / rename / copy and always ask before moving or renaming programs and scripts.

## Real hole found and fixed while doing P6

`coding-agent-adapters` (the agent-watch sidecar's dependency) starts OpenCode with `--dangerously-skip-permissions`
whenever no approval preset is set, and `autonomous` does the same for every CLI (`--full-auto` for Codex).
The sidecar never set a preset, so **"ask opencode to ..." ran with permission checks off**. Also, the
sidecar's own test (`server.test.cjs`) expected token auth, blank-task rejection and exit-with-parent, but
`server.cjs` and `agent_watch.rs` no longer had them (that hardening was lost in the C:-to-D: move), so any
local process could POST `/watch` and start agents.

Fixed: per-run secret (`x-nexus-token`, ring `SystemRandom`, constant-time compare), Host-header check
(DNS rebinding), refuses to start without a token, exits when the parent's stdin pipe closes or the parent
dies, task required, workdir must exist, and **only a vetted `approvalPreset` is forwarded** (`readonly`,
`standard`, `permissive`, `edit`; default `edit` = read and edit files in the project, no commands, no web,
no sub-agents; `autonomous` and anything unknown is refused by the sidecar and coerced to `edit` on the Rust
side). `coder::dispatch_to_agent` refuses any known bypass flag and gives a prompt that starts with `-` a text
lead-in so it cannot be read as an option.

### Windows guidance (no first-party equivalent of bubblewrap/Seatbelt)
Claude Code has no native-Windows sandbox; Codex has a Windows-native one that needs admin setup. The kernel's
path scoping is the boundary for NEXUS's own file actions and is honestly weaker than an OS sandbox. For an agent
that must run commands, run it inside WSL2 or Windows Sandbox on a copy of the project, or keep the default
`edit` preset (no command execution at all).

## Verification (all run on D:\ULTRON)

- `cargo test --lib action_kernel`: 69 tests (paths incl. real junction + 8.3 names, policy, journal, audit, files, resolver, voice, injection corpus, shell-free grep gate).
- **Mutation check** (`scratchpad/mutate_ak.py`): 20 guard mutations (provenance, each forbidden location, roots, caps, delete-asks, TOCTOU, yes-required, purge/restore safety, no-overwrite, everything refusal, ambiguity, confirm origin, stale plan, secrets rule, wipe claim, trailing dots). The first run found **6 survivors** (guards masked by overlapping ones); isolation tests were added and the second run killed all 6. Final: 20/20 killed.
- Sidecar: `node --test server.test.cjs` 4/4 (previously failing for lost hardening).
- Full Rust lib suite (serial, skipping the two Kokoro/bench groups): 1668 passed, 2 failed (the two foreign Spotify tests below). Frontend: `npx tsc --noEmit` clean, vitest 323/323.
- Gates: turn-end OK, repo hygiene OK, no new dead code from the kernel. IPC gate: none of the new `actions_*` commands is flagged; the gate still fails on items that were already failing (`get_return_review` not registered, `agent_watch_start/status`, `speech_stage_snapshot`, unlistened `agent_watch:*` / `ghost:action_receipts` / `return:review` events) which belong to work lost in the move.
- Foreign failures unchanged: `intent_parser::tests::test_launch_spotify`, `test_app_phonetic_mishearings`.

## Not run / limits (stated plainly)

- **Nothing was run through the real app by voice** (no mic, no rebuild): the parser, resolver, plan, confirm, execute, undo, purge and Hub IPC are tested against temp folders; the orchestrator glue, sidebar card and offer window are compile-checked and gate-checked only. Rebuild (`nexus build`) and try: "delete the pdfs in downloads" -> read-back -> "yes" -> "undo that".
- Speaker verification is not strong enough to rely on alone: a delete relies on the spoken confirmation of a resolved preview; an Uncertain voice cannot answer it and a non-voice caller cannot.
- The delete is a move inside the same volume (instant). There is no "send to Recycle Bin" step and no permanent delete from voice; the only permanent removal is the audited age purge of `.nexus-trash` (default 30 days).
- Object resolution is one folder level, name-contains matching; STT mishearings of a file name will not match (asks rather than guesses).
- `D:\` project folders are not default roots: add them in the Hub (the voice name is the folder's last name, e.g. "in the projects folder").
- Policy file edits made by hand are not sanitised at load (only Hub edits are); the engine still re-checks every path itself.
- "undo that" while dictation is on is dictation's command, not the file undo.
