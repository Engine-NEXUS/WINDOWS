# Feature 91 - Reverse Lab (static reverse-engineering triage)

> Say it: **"NEXUS, reverse engineer this app"**, **"what does the file I just downloaded do"**,
> **"turn the latest HAR into an API spec"**. NEXUS reads the file, never runs it, and puts a report in the sidebar.

Research behind it: `docs/research/reverse-engineering/01-04-*.md`. Measured result: `docs/research/reverse-engineering/scorecard-after.md`
(re-run any time with `nexus reverse score`). Change log: `docs/changes/92-reverse-lab-measured-reverse-engineering-feature.md`.

## What it does

| Input | What you get |
|---|---|
| Windows program / DLL / driver (native or .NET) | file facts (arch, subsystem, version info, hardening, signature *presence*), imports by DLL, sections + entropy, interesting strings (URLs, registry keys, paths, PDB), **capability findings** (48 declarative rules mapped to ATT&CK, each with a self-test), **packer/obfuscation verdict**, and an evidence-bound explanation |
| Electron `app.asar` (or an `.exe` with `resources/app.asar` next to it) | package name/version/main, dependencies, native `.node` addons, preload scripts |
| A HAR capture of *your own* session | inferred endpoints (path templates, query params, request/response schemas, bearer/cookie/api-key auth, pagination, GraphQL operation catalogue), an **OpenAPI 3.0** file saved under the app-data `reverse/` folder, **no example values** |

Where "this app" comes from: the foreground window's executable (full path), the newest program in Downloads (<= 30 min), the newest `.har` in Downloads/Desktop (<= 24 h), a path on the clipboard, or an explicit path (`nexus reverse analyse <file>`, the `reverse_analyze_path` IPC command).

## Hard rules (each has a gate in `scripts/reverse_gates.py`)

1. **Never executes the target.** Files are only read as bytes. A grep gate over all Reverse Lab code rejects process-spawn / library-load / injection calls, and the gate proves it can fail by detecting a planted violation first.
2. **Secrets stay local.** HAR values are classified and dropped *before* inference (only names, types and path shapes survive; no examples are ever emitted). Strings in binaries are redacted at extraction (including `user:pw@host` URLs). Tests plant secrets and assert they appear in no report, no JSON and no LLM prompt.
3. **No upload.** The `nexus_reverse` crate has no network dependency. The only request is the optional explanation call to *your own* Groq/Gemini key, carrying a capped (<= 7 000 characters), redacted evidence summary - never the file. Setting `reverseLabExplain` off removes even that.
4. **Authorisation banner** on every report: use only on software, traffic and systems you own or are authorised to analyse.
5. **Refusal.** Requests to crack, keygen, bypass licensing/DRM/activation or write exploits get one fixed spoken refusal; nothing else runs.

## How the explanation is kept honest

The model sees an *evidence list* (every line has an ID like `E-IMP-0003`). Its JSON answer is validated claim by claim and anything unsupported is deleted:

- a claim must cite at least one evidence ID and every ID must exist;
- every API / DLL / file / URL / registry path / quoted string it names must occur in the **cited** evidence (or in what a cited rule matched);
- a claim that names something that exists elsewhere in the evidence but not in its citation is dropped (mis-citation);
- at least half of a claim's content words must come from what it cites (the rest would be the model's own wording);
- behaviour statements (injects, persists, logs keystrokes, contacts a server ...) must cite a capability rule of that category;
- verdicts ("malware", "safe", "trojan" ...) and instruction-like text are never accepted;
- a file flagged as packed is **always** rendered with its limitation first ("the imports and strings describe the unpacking stub, not the real program");
- if the model output is unusable, a deterministic explanation built from rule hits is shown instead.

## Limits you should know

- **Static only.** It cannot say what a program does at run time, who wrote it, or whether it is safe. It gives leads, not verdicts.
- **No decompiler is bundled** (Ghidra needs a JDK, RetDec is 5-6 GB; the product is free/BYOK with a ~700 MB RAM ceiling). .NET gets metadata and strings only.
- **Packed or obfuscated files** are detected heuristically and reported as limited to the stub; the real program stays hidden.
- **HAR inference** needs >= 3 distinct values (plus a structural or response-shape signal) to turn a word-like path segment into a parameter; sparser captures stay literal. WebSockets are listed, not modelled. Schemas show only what appeared in the capture.
- **Voice**: the phrase parser is deliberately narrow (program-like nouns or a HAR). "Analyse repo X" and PR/screen phrases still go to their own commands.
- Authenticode signature *presence* is reported; the chain is not verified.

## Settings (Settings > Audio/Voice section)

`reverseLabEnabled` (default on), `reverseLabExplain` (default on; ignored when no key), `reverseLabMaxFileMb` (default 512, settings.json only).

## Commands

```
nexus reverse analyse <program.exe|.dll|app.asar>   # markdown report, rule-based explanation
nexus reverse har <capture.har>                     # OpenAPI 3.0 YAML
nexus reverse score                                 # measure everything and print the scorecard
python scripts/reverse_corpus.py fetch && python scripts/reverse_corpus.py pack   # build the measurement corpus once (needs upx)
```

## Files

`src-tauri/reverse-core/` (standalone crate: `pe`, `dotnet`, `electron`, `strings`, `packer`, `rules` + `rules/*.yaml`, `evidence`, `redact`, `har`, `explain`, `report`, `analyze`, CLI `nexus-reverse`), `src-tauri/src/reverse/` (`target`, `runner`, `ReverseCenter`), intents in `intent_parser.rs`, routing in `center.rs` / `orchestrator.rs`, `router::route_with_system`, scripts `reverse_corpus.py`, `reverse_eval.py`, `reverse_har_bench.py`, `reverse_gates.py`, `reverse_scorecard.py`.
