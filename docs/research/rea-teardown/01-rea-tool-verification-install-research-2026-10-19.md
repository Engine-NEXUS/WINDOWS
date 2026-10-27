# REA (Reverse Engineer Anything) — Tool Verification & Pinned Install Research

**Date:** 2026-10-19. Status: RESEARCH (nothing installed, nothing built). Companion docs: [02 website observation & law compliance](02-browser-observation-law-compliance-research-2026-10-19.md) · [03 teardown bay & diag design](03-teardown-bay-diag-design-research-2026-10-19.md) · [04 build plan](04-build-plan-research-2026-10-19.md).

**Purpose:** nail down the REA tool's identity, trust posture, version pin, and the exact PATH-independent install/invocation design for NEXUS — before any byte touches disk.

---

## 1. The owner ask (root)

Give NEXUS a reverse-engineering kit: **REA** (`github.com/morluto/rea`, MIT, npm package `rea-agents`). NEXUS uses it on **WEBSITES ONLY**: public sites, open-source code, and the owner's own projects. Install **outside the project** in `~/.nexus-tools/rea`; verify the npm package really points at the official repo *before* installing (copycat packages/repos exist); pin the exact version (never `latest` at runtime); call REA as `node <HOME>/node_modules/rea-agents/scripts/rea.mjs COMMAND --json` so it works when the server starts without the owner's shell PATH.

## 2. Registry verification — evidence table

Verified live on 2026-10-19 with `npm view rea-agents --json` (read-only; no install performed):

| Field | Value read from registry | Verdict vs owner rule |
|---|---|---|
| `name` | `rea-agents` | exact package asked for |
| `dist-tags.latest` | `6.7.0` (published 2026-10-18) | **pin = `rea-agents@6.7.0`** |
| `repository.url` | `git+https://github.com/morluto/rea.git` | ✓ points at `github.com/morluto/rea` — official, not a copycat |
| `bugs.url` | `https://github.com/morluto/rea/issues` | ✓ same origin |
| `homepage` | `https://github.com/morluto/rea#readme` | ✓ |
| `license` | `MIT` | ✓ owner asked MIT; repo `LICENSE` is MIT (third-party deps carry their own licenses, inventoried in `THIRD_PARTY_NOTICES.md`) |
| `engines` | `^22.19.0 \|\| ^24.11.0 \|\| >=26.0.0` | ✓ matches "Node.js 22.19+ (or 24.11+)" |
| `bin` | `rea` → `scripts/rea.mjs`, `rea-agents` → `scripts/rea.mjs` | ✓ supports the `node .../scripts/rea.mjs COMMAND --json` invocation |
| `type` / `imports` | `module`; `#javascript-analysis-worker` → `dist/composition/javascriptAnalysisWorkerProcess.js` | prebuilt ESM runtime; no build step needed |
| `dist.unpackedSize` / `fileCount` | 28,575,703 bytes / 1,775 files | size estimate for the tool dir |
| `dist.tarball` | `https://registry.npmjs.org/rea-agents/-/rea-agents-6.7.0.tgz` | source of truth for install |
| publisher | `GitHub Actions <npm-oidc-no-reply@github.com>`, `_from: file:/home/runner/work/rea/rea/.cache/verified-package/package.tgz` | published from CI |
| attestations | `slsa.dev/provenance/v1` present for 6.7.0 | supply-chain provenance signal |
| `optionalDependencies` | `frida@17.23.2` (with `allowScripts` entry) | see §4 `--ignore-scripts` analysis |
| maintainers | `morluto <morluto@hotmail.com>` (single) | recorded |

## 3. Version landscape → the pin decision

Published 0.2.1 (2026-07-12) through 6.7.0 (2026-10-18): a rapid, actively maintained series (3,719 commits on GitHub main; 320k stars reported by GitHub page). Notable: there is **no 6.6.0** on the registry (list jumps 6.5.0 → 6.7.0) — evidence the pin must be an **exact string**, not a semver range.

- Install pin: **`rea-agents@6.7.0`**, `--save-exact` writes `"rea-agents": "6.7.0"` into `~/.nexus-tools/rea/package.json` (no `^`), so any later `npm ci`/`npm install` in that dir reproduces the identical version.
- Runtime never resolves `latest`: the bay calls the file path directly (`node .../node_modules/rea-agents/scripts/rea.mjs ...`), so node_modules content *is* the version — validated at every diag (see doc 03 §6).
- Upgrade policy (research proposal, not built): new pin = owner-visible step with a fresh `npm view rea-agents repository.url` check, not an automatic one.

## 4. `--ignore-scripts` semantics (why it is the right install)

From the `scripts` map in the registry metadata: the package carries lifecycle scripts — `prepare` (`node scripts/prepare.mjs`), `prepack`, `postpack`-family, plus heavy dev scripts. With **`--ignore-scripts`**:

1. None of npm lifecycle scripts run on install. The published tarball is **prebuilt and runtime-complete** (the `bin` target `scripts/rea.mjs` exists as a file; `dist/` ships in the tarball; `type: module` ESM entries self-contained). The npm release checked in REA docs ("Released package and main" section of installation.md) confirms dry-run setup plans work straight from the published package.
2. **Blocks `frida@17.23.2` postinstall** (the only optional dependency with native download scripts). Frida powers REA's *dynamic instrumentation* surface — explicitly out of scope for observe-only website work. Not downloading its native binaries is a feature, not a loss, for NEXUS.
3. REA's own docs bless this posture (installation.md uses `bun add --global --ignore-scripts` for Bun global installs); the equivalent npm flag is the supported analog.
4. Known trade-off recorded: with scripts ignored, npm's script-approval telemetry is bypassed — but the only allow-listed script WAS frida's, already undesirable here.

## 5. Location design — outside the project

```
~/.nexus-tools/rea/                    ← the tool home (NEVER inside the NEXUS repo)
  package.json                         ← written by npm init -y; holds the exact pin
  node_modules/rea-agents/…            ← the install itself (1,775 files, 28.6 MB unpacked)
  node_modules/rea-agents/scripts/rea.mjs   ← the canonical entrypoint
  node_modules/rea-agents/package.json ← read at every diag for version + repository.url guard
```

Reasons recorded:
- **Isolation**: nothing about the NEXUS project's `node_modules`, lockfiles, or npm config touches the tool (and vice versa); the tool survives project rebuilds/re-clones.
- **PATH independence**: the bay references absolute file paths under this dir, so a Windows service that starts NEXUS without the interactive shell's PATH still finds REA.
- **Revocability**: `rmdir /s` on the tool dir fully uninstalls; no global npm state, no registry pollution.

## 6. Invocation design — PATH-independent Node

Owner rule: call REA as `node $HOME/node_modules/rea-agents/scripts/rea.mjs COMMAND --json` — i.e., **always via `node <abs path to rea.mjs>`**, never via the `rea` on PATH (that shim only exists because npm wrote it into a global prefix; in `~/.nexus-tools/rea` there IS no PATH shim at all).

Node discovery ladder for teardown (verified mix: current machine has `node` on PATH at `C:\Program Files\nodejs\node.exe`, v24.12.0):

**Windows candidates (in order):**
1. `node` on the server process's own PATH (`shutil.which("node")`)
2. `C:\Program Files\nodejs\node.exe`
3. `C:\Program Files (x86)\nodejs\node.exe`
4. `%LOCALAPPDATA%\Programs\nodejs\node.exe`
5. `%SOFTROOT%\...` registry entries are out (see doc 03 §12 open questions — winreg being stdlib makes this *possible*, flagged optional)

**macOS candidates:** `node` on PATH → `/usr/local/bin/node` → `/opt/homebrew/bin/node` → `~/.nvm/versions/node/*/bin/node` glob → fnm/volta dirs.

**Version gate:** parse the output of `node --version` (`vMAJOR.MINOR.PATCH`), then match against the engines rule **`^22.19.0 || ^24.11.0 || >=26.0.0`** — i.e. `(22, ≥19.0) or (24, ≥11.0) or (≥26)`. Node 24.12.0 on this machine satisfies `^24.11.0` ✓.

## 7. CLI contract NEXUS will script against (from docs/cli.md)

| Aspect | Fact |
|---|---|
| Default output | **TOON** terminal format; `--json` selects JSON CLI representation (the global `--json` flag is the flag NEXUS always passes) |
| `--json` rendering | indented on a terminal, **compact when piped** (good for parsing; pipe `jq .` only if a human reads it) |
| Exit statuses | `0` = operation completed (result may carry partial evidence/warnings — read the payload, not just the code); `1` = could not complete (structured failure info when available); `128+N` = signal-derived (130 SIGINT / 143 SIGTERM) |
| `SIGPIPE` | piped reader closing early (`rea … \| head`) exits 141; the operation may have completed — scripted pipelines must not treat 141 as data loss |
| `mcp` subcommand | `node rea.mjs mcp` starts the MCP server (`npm start` equivalent); NEXUS uses **CLI commands**, not MCP, in v1 |
| capability probe | `node rea.mjs capabilities --json` describes binary-session providers and auxiliary capabilities — the cheap "is REA alive" smoke for diag (fast, starts no provider) |
| full audit | `node rea.mjs doctor --json` audits registrations/skill/providers — read-only but slower; diag keeps it opt-in |
| setup family | `rea setup --dry-run` exits 0 with status `planned`; interactive setup requires TTYs; cancellation exits 0 — scripting around setup is possible without touching agent configs (NEXUS plans *not* to use `setup`; it registers MCP into agent configs, which NEXUS does not need for CLI-only use) |

## 8. Exact install command sequence (planned, not yet run)

```powershell
# 0. Already done 2026-10-19 (research verification, read-only):
npm view rea-agents --json          # → repository.url = git+https://github.com/morluto/rea.git ✓

# 1. Isolated tool home, outside every project:
New-Item -ItemType Directory -Force "$env:USERPROFILE\.nexus-tools\rea"

# 2. Install pinned + scriptless:
Set-Location "$env:USERPROFILE\.nexus-tools\rea"
npm init -y
npm install "rea-agents@6.7.0" --save-exact --ignore-scripts

# 3. PATH-independent smoke:
node "$env:USERPROFILE\.nexus-tools\rea\node_modules\rea-agents\scripts\rea.mjs" capabilities --json
```

macOS analog: same three steps with `$HOME/.nexus-tools/rea`.

## 9. Honest-claims ledger (research → build)

| Claim allowed | Claim not allowed (and why) |
|---|---|
| "Pin verified against the official repo at install time (repository.url matched)" | "npm tarball bytes byte-identical to repo main" — installation.md explicitly notes *the same version string in a dev checkout does not establish that its bytes match the npm tarball*; tree-hash of the shipped tarball recorded at install is the honest equivalent |
| "Runs via explicit node + absolute path, no PATH dependency" | "Works on any Node 18" — engines gate says no |
| "No install scripts ran (npm audit of --ignore-scripts)" | "Zero native code present" — the tarball ships WASM/native assets statically; we enumerate at install if needed |

## 10. Open questions (not verified in research)

1. **Registry↔repo drift window**: registry had 6.7.0 published 2026-10-18; REA docs' "Released package and main" checkpoint was 6.4.0 (2026-10-11) — docs lag releases; features newer than the checked release may differ. Re-check the browser docs against the actually-installed 6.7.0 behavior in T4 (doc 04).
2. `npm ci`-style reproducible reinstalls from the tool dir's fresh `package.json` — planned but not exercised.
3. Node 26.x LTS compatibility — engine says `>=26.0.0` allowed; no machine has tested it.
4. Windows `where.exe node` returns multiple PATH entries — first-match rule to be sustained by the bay's `shutil.which` (documented, tests to be written).

## 11. Sources (checked 2026-10-19)

- **npm registry (live `npm view rea-agents --json`)** — every field in §2 (id, dist-tags, versions list, time, repository.url, engines, bin, dist/integrity/attestations, maintainers, scripts map, dependencies/optionalDependencies, allowScripts)
- **`github.com/morluto/rea` README (main branch, fetched 2026-10-19)** — quick start (`npx rea-agents setup`), official-source warning ("A similar repository name or copied README does not establish that a download is official"), Node requirements line ("Node.js 22.x (>=22.19), 24.x (>=24.11), or 26+"), update/update-retention notes, website targets row of "What you can analyze" table, MIT license section, responsible-use section pointer to DISCLAIMER.md
- **`docs/installation.md` (raw fetch)** — setup/dry-run/status semantics, `.rea.backup` behavior, "Released package and main" release-boundary text, Bun `--ignore-scripts` precedent, supported agents table (NEXUS uses none of them — CLI only)
- **`docs/cli.md` (raw fetch)** — §7 facts: TOON/­`--json`, exit-status table incl. 141 SIGPIPE handling, `capabilities`/`doctor`, `analyze-javascript-application` and evidence save/reuse pattern, snapshot guidance
- **Repository page metadata** — 3,719 commits, 320.2k stars / 81.2k forks, MIT license tag, "rea.tools" website link
- **Local machine checks** — `node --version` → v24.12.0 at `C:\Program Files\nodejs\node.exe` (PATH); `where.exe node`; Chrome probes (§ doc 03 §5)
