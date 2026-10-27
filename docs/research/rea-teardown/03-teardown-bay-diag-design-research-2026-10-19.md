# NEXUS Teardown Bay & /teardown/diag — Design Research

**Date:** 2026-10-19. Status: RESEARCH (no code written). Companion docs: [01 tool verification](01-rea-tool-verification-install-research-2026-10-19.md) · [02 law compliance](02-browser-observation-law-compliance-research-2026-10-19.md) · [04 build plan](04-build-plan-research-2026-10-19.md).

**Purpose:** map the "Teardown Bay" (`teardown.py`, Python stdlib only, soft-imported, `GET /teardown/diag`) onto NEXUS's actual FastAPI sidecar conventions so the build lands in-repo culture, plus the diag response contract, discovery ladders, in-character failure replies, and crash-resistance rules.

---

## 1. Host anatomy — where the bay attaches (verified from repo, 2026-10-19)

The Python HTTP sidecar is `D:\ultron\server\sidecar\sidecar.py` ("NEXUS WSS Bridge", FastAPI, version header 0.3.0). Verified structure:

| Fact | Location (file:line) | Detail |
|---|---|---|
| FastAPI app object | `sidecar.py:68` | `app = FastAPI(title="NEXUS WSS Bridge", version="0.3.0")` |
| Router composition precedent | `sidecar.py:69` | `app.include_router(oauth_router)` — bays register sub-routers, not just inline routes |
| Minimal GET health precedent | `sidecar.py:93-99` | `@app.get("/health")` returns flat `JSONResponse({"ok": True, "sessions": …, "protocol": "text-only"})` — the shape `/teardown/diag` mirrors |
| Text-only WebSocket protocol | `sidecar.py:1-30` | Audio never crosses network; text in/text out — one more reason the REA bay returning JSON text inline is house-style |
| Entry point | `sidecar.py:391-398` | `uvicorn.run("sidecar:app", host=os.getenv("SIDECAR_HOST","0.0.0.0"), port=int(os.getenv("SIDECAR_PORT","41098")))` |
| Auth posture | `sidecar.py:55`, `oauth.py` routes | `NEXUS_SIDECAR_TOKEN` gates sensitive endpoints; `/health` is unauthenticated — diag follows the health-style pattern (returns no secrets; see §9) |
| The other "server.py" | `server/youtube/server.py` | The youtube bay — precedent that a named bay can own a `server.py` module name; but teardown's natural home is **inside the sidecar package** (same uvicorn process) |

**Supporting environment verified live:** NLU server `server/nlu_server.py:180,215` (FastAPI + `/health`) and STT server `server/stt_server.py:69,132` (FastAPI + `/health` + WebSocket) — so *every* NEXUS Python HTTP surface is FastAPI, and the bay rides one of them; no new server process needed for the diag v1.

## 2. Soft-import contract (the part that keeps "NEXUS boots fine without it")

Planned insertion point in `sidecar.py` module top (after existing imports, before `app =`):

```python
# --- Teardown bay (optional, stdlib-only): degrades gracefully ---
try:
    from . import teardown           # from .  → import failure cannot hurt the bridge
except Exception:                     # noqa: bare-except on purpose: bay must NEVER break boot
    teardown = None

app = FastAPI(title="NEXUS WSS Bridge", version="0.3.0")
app.include_router(oauth_router)

if teardown is not None:
    app.include_router(teardown.router)      # bay owns its own APIRouter
else:
    @app.get("/teardown/diag")               # explicit degraded route, see §7
    async def teardown_diag_degraded() -> JSONResponse:
        return JSONResponse({"ready": False,
                             "problems": ["teardown bay failed to load — REA diag unavailable"]})
```

Design decisions recorded:

1. `teardown.py` itself **never imports fastapi** — it builds a `fastapi.APIRouter` *sub-class-free* via a lazy import inside a factory? No — cleaner: teardown.py holds only a plain dict-returning function; the fastapi route is defined in sidecar.py (keeps stdlib-only literally true inside the bay). Alternative accepted form: use `fastapi.APIRouter` in teardown.py conditioned on import — rejected (violates "stdlib only"). Final shape: **teardown.py = pure diag logic; sidecar.py holds the one route handler that calls it.**
2. `except Exception` (not just `ImportError`) — a bay that half-imports (log spam, missing env parsing) must also degrade, per "nothing crashes".
3. Bay presence is **silent** in logs beyond one `log.info("teardown bay loaded/not loaded")` line — no boot-time subprocess spawns (nothing may slow NEXUS startup).

## 3. `/teardown/diag` response contract

Single GET, JSON (flat scalars, nested sub-objects where warranted). Draft contract (build finalizes):

```jsonc
{
  "ready": true,                        // ALL of the §6 checks pass
  "pinned_version": "6.7.0",            // written constant in teardown.py
  "installed_version": "6.7.0",         // read from node_modules/rea-agents/package.json
  "repo_url_ok": true,                  // installed package's repository.url contains github.com/morluto/rea
  "rea_script_present": true,           // node_modules/rea-agents/scripts/rea.mjs exists
  "node": { "found": true, "version": "24.12.0", "path": "C:\\Program Files\\nodejs\\node.exe",
            "engine_ok": true },        // satisfies ^22.19.0 || ^24.11.0 || >=26.0.0
  "chrome": { "found": true, "path": "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe" },
  "problems": [],                       // plain-English list, one item per failed check
  "advice": null                        // in-character one-liner when !ready (see §8)
}
```

Rules:

- `problems` uses **plain-English** sentences, not error codes ("Node 24.5.1 is older than REA's 24.11+ requirement").
- `ready` is `false` when any primary check fails — but the response is still HTTP 200 with the JSON; the "one line, in character" clause is enforced at the * Jarvis phrasing* layer downstream, not by HTTP status.
- Live smoke probe (§7) only runs on the **`?full=1`** query param (slow path); default diag is filesystem + `node --version` only — fast (single-digit ms + one 100 ms-bounded subprocess).

## 4. Check ladder (what the diag actually inspects)

| # | Check | Method (stdlib) | Failure wording seed |
|---|---|---|---|
| 1 | Node present | PATH `shutil.which("node")`, else ordered candidate paths (doc 01 §6 lists), else parse `registry` install marker on Windows via `os.environ` fallbacks | "Node.js is not on PATH and none of the common install folders had it." |
| 2 | Node version | `subprocess.run([node, "--version"], timeout=10, capture_output=True)`; regex `^v(\d+)\.(\d+)\.(\d+)$`; engine match `(maj,mnipat) in [(22,19,0),(24,11,0)] or maj>=26` comparison logic | "Node 24.5.1 is older than REA's 24.11+ floor." |
| 3 | Tool dir exists | `os.path.isdir(os.path.expanduser("~/.nexus-tools/rea"))` | "The REA tooling folder ~/.nexus-tools/rea doesn't exist yet — run the pinned install." |
| 4 | Package manifest | read `node_modules/rea-agents/package.json`, `json.loads`; version field | "rea-agents isn't installed inside ~/.nexus-tools/rea/node_modules." |
| 5 | Pin match | `installed_version == PINNED_REA_VERSION` (constant `"6.7.0"`) | "Installed REA is 6.7.1 but NEXUS is pinned to 6.7.0 — reinstall the pinned version." |
| 6 | Repo guard | `contains(installed_package["repository"]["url"], "github.com/morluto/rea")` | "The installed rea-agents doesn't come from github.com/morluto/rea — that's a copycat risk; reinstall from the official source." |
| 7 | Entry script | `os.path.isfile(.../scripts/rea.mjs)` | "rea.mjs is missing from the install — reinstall." |
| 8 | Chrome present | ordered candidate paths inline (§5) | "Google Chrome wasn't found in the usual locations." |
| 9 | (opt, `?full=1`) | `subprocess.run([node, rea.mjs, "capabilities", "--json"], timeout=30)`; exit==0 + parse | "REA didn't answer its capabilities probe (<reason>)." |

Every check isolated (`try/except` around each), the diag function never raises; failed checks append `problems` and force `ready=False` while still returning all other findings.

## 5. Chrome discovery ladder (filesystem-first; both platforms)

Ordered candidates, first hit wins (verified hit on this machine is starred):

**Windows**
1. ★ `C:\Program Files\Google\Chrome\Application\chrome.exe` (verified present 2026-10-19)
2. `C:\Program Files (x86)\Google\Chrome\Application\chrome.exe` (not present here)
3. `%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe` (per-user install; not present here)

**macOS**
1. `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` (canonical; REA docs use this exact path in their macOS examples)
2. `~/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`

**Linux (echo-family completeness, low priority):** `google-chrome`, `chromium`, `chromium-browser` on PATH, `/usr/bin/…` — REA docs mention Fedora's `chromium-browser` naming.

Fallback: `winreg` ARE stdlib on Windows — registry keys (`Software\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe`) readable at zero dependencies; **deferred to T5 hardening** (open question §10.2) so v1 stays lean and testable.

## 6. Node engine match — exact rule

```
def engine_ok(major, minor, patch):
    return (major == 22 and (minor, patch) >= (19, 0)
            or major == 24 and (minor, patch) >= (11, 0)
            or major >= 26)
```

- Verified live: `v24.12.0` → `(24,12,0) >= (24,11,0)` ✓ (`engine_ok: true`).
- Node 25.x (odd line, EOL soon) fails all arms — correctly rejected, matching the engines string `^22.19.0 || ^24.11.0 || >=26.0.0`.
- Unparseable output (`v0.x.x`, "command not found" tongue) → `found: true` but `engine_ok: false` + problem line ("couldn't read a valid version from node --version") — report honestly rather than guess.

## 7. Live smoke probe design (opt-in `?full=1`)

`node <REA_HOME>/node_modules/rea-agents/scripts/rea.mjs capabilities --json`

- chosen because `capabilities --json` starts **no analysis engine**, per docs/cli.md; exit 0 expected; output validated as JSON (`capabilities`/`providers` keys) — a copycat pretending to be rea would either fail exit codes or structure, another passive guard.
- timeout 30 s; on timeout the probe reports failure in `problems` but must never crash the endpoint — one more `try/except` ring.
- `doctor --json` deliberately **not** a default check (read-only but heavier; audits registrations not needed for PURE-mode www-only NEXUS): available only if a debug flag is ever added.

## 8. In-character failure replies (the "answers in one line, in character" clause)

NEXUS's sidecar ack phrases are fixed persona lines (sidecar.py:60-66): *"On it, sir."* register. The diag advice drawer maps each problem class onto a pre-written one-liner (kept ≤ 1 sentence, in character, stating what to install, per the owner's acceptance rule). Draft catalog:

| Problems contain | Advice line |
|---|---|
| REA missing | "On it, sir — REA isn't installed yet. Run `npm install rea-agents@6.7.0 --save-exact --ignore-scripts` inside ~/.nexus-tools/rea and we're in business." |
| Version mismatch | "Sir, the installed REA is <installed> but this build is pinned to <pinned>. Reinstating the exact pin is a one-liner in the same folder." |
| Copycat repo | "Steady on, sir — that rea-agents build doesn't trace back to github.com/morluto/rea. Same install command after a clean `rmdir /s` of ~/.nexus-tools/rea." |
| Node old | "Sir, REA wants Node 22.19+ or 24.11+; this machine reports <version>. A Node upgrade and we're past it." |
| Chrome missing | "No Google Chrome in the usual spots, sir — installing Chrome is the only REA-roadblock left." |
| everything OK | "All clear, sir — REA 6.7.0 is pinned, Node <v> qualifies, and Chrome is in place." |

(TTS-side rendering is downstream; the bay only returns the string, one per diag, so NEXUS line-length budget rules already in force elsewhere apply.)

## 9. Security & privacy posture of the endpoint

- Returns **no file contents**, no env, no tokens — paths and versions only.
- Unauthenticated like `/health`, but the sidecar front ends it with loopback-CA'd HTTPS in production (Caddy convention noted in sidecar docstring) — no REA capability is reachable through the diag itself, it never *runs* REA analysis commands; the smoke probe is read-only.
- The bay never touches network; requests land on localhost only (sidecar binds 0.0.0.0 — Caddy fronts public traffic; noted as existing risk, not introduced here).
- Clear `Origin`-tracking of nothing: no cookies, no session state.

## 10. Open questions (not verified)

1. **Sidecar port ownership** — `SIDECAR_PORT=41098` is the WSS bridge; adding `/teardown/diag` to it is the least-ceremony route (research default), but a dedicated lightweight bay-port (new `TEARDOWN_PORT`, stdlib `http.server`-based, still soft) is a valid alternative if the worker/sidecar is later split. Decide at T1.
2. **winreg fallback for Chrome discovery** — zero-dependency but Windows-only; de-scoped from v1 to keep the cross-platform ladder unit-testable (§5).
3. **`?full=1` concurrency** — if two diag calls race, the capability probe runs twice (wasteful but harmless; a tiny asyncio lock in sidecar.py can serialize it, deferred unless telemetry shows contention).
4. **Observation-window cap** — website sessions driven by teardown (the *harder* bay work, doc 02 §6 T-steps) may need a configurable max `--observation-ms`; parked as a teardown config constant to settle in build.

## 11. Sources (checked 2026-10-19)

- `D:\ultron\server\sidecar\sidecar.py` lines 1-120 & 370-398 (interviewed directly, this session) — FastAPI app shape, router include, `/health` precedent, ack-phrase persona, env config, uvicorn entry
- `D:\ultron\server\nlu_server.py` (FastAPI, `/health` at :215), `D:\ultron\server\stt_server.py:69,132` — cross-check that FastAPI+health is the house pattern
- REA `docs/cli.md` — `capabilities`/`doctor` selection rationale, `--json` output rules, exit statuses
- REA `docs/browser-observation.md` — start-a-browser flags (loopback port + user-data-dir), Chrome discovery reasoning, macOS path spelling
- REA `docs/installation.md` — `--ignore-scripts` precedent, release-boundary honest framing
- npm registry metadata (live `npm view rea-agents --json`) — engines string (source of the engine-match rule), scripts map (why --ignore-scripts matters), dist layout (why the file checks in §4 suffice)
- Local machine probes — `node --version` v24.12.0, `where.exe node`, three Windows Chrome path `Test-Path` results
