# REA Teardown Build Plan (T0–T5)

**Date:** 2026-10-19. Status: PLAN (nothing built). Companion docs: [01 tool](01-rea-tool-verification-install-research-2026-10-19.md) · [02 law compliance](02-browser-observation-law-compliance-research-2026-10-19.md) · [03 bay & diag](03-teardown-bay-diag-design-research-2026-10-19.md).

**Ground rules (house style):** every phase ends with tests, not claims; nothing live-run before the T4 gate; kill-switches preserved; every session appends AGENTS.md with Built/Verify/Not-run-live lines.

---

## 1. Roadmap (dependencies explicit)

| Order | Phase | Deliverable | Depends on | Gate |
|---|---|---|---|---|
| 1 | **T0 spec sign-off** | this doc set reviewed; pinned version confirmed `6.7.0` by owner; route surface list sealed (doc 02 §3) | — | owner approval — no code |
| 2 | **T1 install + pin** | `~/.nexus-tools/rea` created on dev machine; `npm install rea-agents@6.7.0 --save-exact --ignore-scripts`; tarball SHA-512 recorded in `docs/research/rea-teardown/data/install-log.md` | T0 | `npm list` exact-version match; `node rea.mjs capabilities --json` exit 0; clean-project `rmdir` + reinstall drill |
| 3 | **T2 diag core** | `server/sidecar/teardown.py` (stdlib-only checks 1-8 of doc 03 §4) + soft-import + `/teardown/diag` route in sidecar.py | T0 | unit tests green — diag JSON contract snapshot, engine-match fuzz, no-boot-failure fuzz (module removed → `degraded` JSON, sidecar boots unchanged) |
| 4 | **T3 law enforcement layer** | teardown session service spawning Chrome (`--remote-debugging-port` + temp profile, loopback-only), command whitelist = doc 02 §3 passive set, route guard rejects non-loopback endpoints and any scenario-provider verb; one-line in-character advice catalog (doc 03 §8) wired into responses | T1, T2 | whitelist **negative** tests pass (scenario command refused; private/auth targets like `http://localhost:9999/admin`, `user:pass@…` rejected as invalid CDP endpoint/credential-bearing inputs); Chrome spawn/teardown tested w/ temp profile cleanup |
| 5 | **T4 live verification (first real runs)** | end-to-end on ≥1 public website + 1 owner project (local dev server over loopback + owner-owned open-source clone): list browse targets, inspect page, screenshot, bundle analyse, session observe, compare captures; re-verify install drill on a *second* machine (Mac or Windows) | T2, T3 | `/teardown/diag` reports `ready:true` live; **the owner-visible VERIFY output captured in AGENTS.md**: pinned version shown + the exact commands of T1; evidence JSONs shrunk-and-kept for one small public page as research artifacts |
| 6 | **T5 hardening** | registry-mismatch & copycat self-repair drills; T4 corpus replayed after **next REA release** upgrade test (whitelist re-check); winreg fallback option revisit; docs synced (update this suite + `docs/research/README.md` + `THIRD_PARTY_NOTICES` if worker bundles it) | T4 | every failure-mode returns one line (nothing crashes); version bump never silently executed — flagged to owner with re-pinned doc |

Sizing note (honest): T1-T4 is an evening of work on a single machine; T5 is where the multi-platform + upgrade-guard story matures across several builds, tracking REA's weekly release cadence.

## 2. Module & file map (planned layout)

```
~/.nexus-tools/rea/                                   (tool home — outside repo, doc 01 §5)
docs/research/rea-teardown/data/                      evidence JSONs, install log (research artifacts)
server/sidecar/teardown.py                            stdlib-only bay: diag pillar + helpers
server/sidecar/sidecar.py                             + soft-import block + 2 routes (/teardown/diag [+ later /teardown/session])
frontend/src/                                         (later phase) one Hub card reading /teardown/diag
tests (existing pytest conventions)                   diag + whitelist + engine-match suites
```

Anywhere a REA invocation happens, the template is:

```python
node_path = _find_node()                       # doc 03 §4 ladder
rea_script = os.path.join(REA_HOME, "node_modules", "rea-agents", "scripts", "rea.mjs")
subprocess.run([node_path, rea_script, *cmd, "--json"], capture_output=True, timeout=…)
```

No `npx`, no `rea` on PATH, no `latest` — the three anti-patterns permanently banned in teardown code (grep-gate test asserts their absence).

## 3. Budgets

| Item | Target |
|---|---|
| `/teardown/diag` default latency | < 250 ms p50 (filesystem + one 10 s-capped `node --version` subprocess) |
| diag smoke (`?full=1`) | < 5 s (30 s hard cap; `capabilities --json` is startup-cheap per docs) |
| teardown bay CPU at idle | 0 (no timers, no warm subprocesses; sidecar untouched unless route hit) |
| Evidence storage | JSON artifacts under `docs/research/rea-teardown/data/` only after T4 — each ≤ 2 MB per session; nothing auto-purged v1 |
| Chrome temp profile lifetime | spawned per session, deleted in the finally-block; never a persistent profile dir reused across sessions |

## 4. Testing matrix (each cell = a test that must pass before the next phase)

1. **diag golden responses** — `ready:true` success snapshot; each of the 8 failure classes with its exact `problems`/`advice` text (doc 03 §8).
2. **engine-match fuzz** — `(22,18,9)`, `(22,19,0)`, `(24,10,9)`, `(24,11,0)`, `(25,3,0)`, `(26,0,0)`, `(30,0,0)`, garbage strings → expected booleans.
3. **no-boot-failure** — teardown.py module renamed mid-boot (import error) → sidecar still boots; `/teardown/diag` returns the degraded shape.
4. **whitelist negative suite** (T3) — scenario commands, native-provider commands, and non-http/ws endpoints rejected at the bay API *before* any subprocess; no `Runtime.evaluate`/input vocabulary exists in the bay's builder.
5. **temp-profile hygiene** (T3) — Chrome spawn/kill drill; assert the `--user-data-dir` directory does not survive process exit.
6. **pin tamper** (T5) — sed the installed package's version to 9.9.9 → diag flips `ready:false` with the pin mismatch line, then restored.
7. **repo tamper** (T5) — rewrite installed `repository.url` to a github.com/evil/rea → diag catches the copycat class instantly.
8. **grep-gates** — no `npx rea` and no `rea-agents@latest` in `server/` sources; no `winreg`, `psutil`, `fastapi` imports inside teardown.py.

## 5. Threat-model deltas in scope of the bay (short list)

- **Copycat package guard**: dual — install-time `npm view repository.url` (done in research, repeated at every install via doc 01 §8 step 0) and runtime diag check #6 (every diag fetches it from the installed `package.json`).
- **Wrong-TLS/pi priv escalation through diag**: impossible by construction — diag returns version/path/boolean facts only, and its only subprocess probes are `node --version` and `rea.mjs capabilities --json`.
- **Scenario-provider leakage**: enforced by whitelist + negative tests (T3) + absence-of-API-surface check in T4 replay, and re-asserted at every REA version bump in T5 (the only surface in the package family that can click/type/change).
- **Loopback-only browser attach**: Chrome spawned with `--remote-debugging-port` on localhost by construction; non-loopback endpoints already rejected by REA itself (doc 02 §2 fact 2) — the bay never passes anything else.

## 6. Acceptance summary (one line)

*"On a clean machine, `GET /teardown/diag` returns `ready:true` with pinned REA 6.7.0, Node version and Chrome path — and when anything is missing it answers in one line, in character, telling exactly what to install, every time, with zero crashes."*

## 7. AGENTS.md entry template (paste-ready per session)

```
## REA teardown — <phase names> (YYYY-MM-DD)
- **Built**: <modules, routes, flags; nothing live-run yet>
- **Verify**: <pytest suites, pass counts; diag snapshot diff; whitelist negative tests>
- **Not run**: <live diag on dev machine; scenario-provider negative test if T3 not in>
- **Docs**: docs/research/rea-teardown/01-04 (sources checked <date>)
```

## 8. Open decision (owner, at T0)

Pinned version **6.7.0**(current, published 2026-10-18) is proposed. Owner may instead pick a later tag at build time — the pin mechanics (doc 01 §3/§8) don't change, only the constant `PINNED_REA_VERSION` in teardown.py and diag's comparison; **every new pin re-runs the `npm view repository.url` gate first** (the owner's standing anti-copycat rule).
