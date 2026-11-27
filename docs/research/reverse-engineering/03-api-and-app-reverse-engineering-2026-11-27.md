# Reverse-engineering a website / app API and UI — open-source landscape

Research date 2026-11-27, primary sources only (READMEs/docs); "not verified" means the page did not answer. No GitHub issues were read, so failure modes come from READMEs.

| Project | Stars | License | Input → output | Templating / auth / redaction |
|---|---|---|---|---|
| [alufers/mitmproxy2swagger](https://github.com/alufers/mitmproxy2swagger) | 9.7k | MIT | mitmproxy flows or HAR → OpenAPI 3.0 | **manual curation step**: first pass lists paths prefixed `ignore:`, user un-ignores and hand-edits `{id}`; greedy, first match wins. No auth handling. `--examples/--headers` may leak tokens/PII; no redaction option |
| [Arkptz/mitm2openapi](https://github.com/Arkptz/mitm2openapi) | 6 | MIT | flows v19–21 / HAR 1.2 → OpenAPI 3.0; single Rust binary (Windows build) | heuristics detect UUID/hex/base58/slug segments + cross-request variability; `--redact-patterns/--redact-fields`, `--max-examples` |
| [spectral-mcp/spectral](https://github.com/spectral-mcp/spectral) | 125 | MIT | Chrome extension or MITM → **MCP tools**, OpenAPI 3.1, GraphQL SDL | LLM used only at analysis/self-repair time (Anthropic/OpenAI/OpenRouter/Ollama/OpenAI-compatible); detects auth flow, generates login script; redaction not described |
| [jasonwcfan/web2sdk](https://github.com/jasonwcfan/web2sdk) | 22 | MIT | HAR → OpenAPI + Python SDK | **templated paths are "planned", not supported**; basic/bearer only |
| [4LAU/apisniff](https://github.com/4LAU/apisniff) | 0 | MIT | live capture / HAR / Burp XML → OpenAPI 3.0.3, GraphQL catalog, replay drift checks | secrets bundles owner-only; **guardrail quoted: "Only run apisniff against systems you own, administer, or have explicit permission to test."**; WebSocket frames only via CDP mode |
| [kalil0321/reverse-api-engineer](https://github.com/kalil0321/reverse-api-engineer) | 4.4k | MIT | browser session HAR → typed client in 10 languages (LLM-written) | keeps the user's real Chrome session; "review before executing"; bot-detection can block capture |
| [mitmproxy/mitmproxy](https://github.com/mitmproxy/mitmproxy) | 47.2k | MIT | proxy (HTTP/1/2, WebSockets) | HAR export / Windows support **not verified** (README silent) |
| [httptoolkit](https://github.com/httptoolkit/httptoolkit) | 3.7k | AGPL/Apache/MIT by repo | interactive proxy | no HAR/OpenAPI export described |
| [zaproxy](https://github.com/zaproxy/zaproxy) | 16.3k | Apache-2.0 | scanner; OpenAPI/Spider add-ons exist | add-on behaviour not read |
| not verified | — | — | AndrewWalsh/openapi-devtools (404), har-to-openapi (403) | — |

## Inspecting a running Windows app's structure (UI side)
- **UI Automation tree**: [pywinauto](https://github.com/pywinauto/pywinauto) (BSD-3, `print_control_identifiers()` text dump), [FlaUI](https://github.com/FlaUI/FlaUI) (MIT, .NET library), Microsoft *Inspect* (ships with Windows SDK, labelled legacy), [Accessibility Insights for Windows](https://github.com/microsoft/accessibility-insights-windows) (an accessibility checker, not a tree exporter; opt-in telemetry). NEXUS already depends on the Rust `uiautomation` crate.
- **Electron apps**: [electron/asar](https://github.com/electron/asar) (MIT; `list`, `extract`; no encryption claimed); `--inspect[=port]` debugging documented (remote-debugging-port not verified in the fetched page).

## Failure modes of HAR→OpenAPI inference (as the READMEs state them)
Path-parameter collapsing (manual in mitmproxy2swagger, heuristic in mitm2openapi, unsupported in web2sdk) · auth (basic/bearer only in web2sdk; cookies only in apisniff proxy mode; OAuth refresh "planned") · GraphQL (not addressed by the two classics; spectral/apisniff do) · WebSockets (apisniff: CDP only) · pagination (undiscussed everywhere) · coverage ("a single request might not capture all possible schemas") · scope (prefix filters must cover the target) · sensitive data (examples/headers can contain tokens; redaction only in mitm2openapi).

## Ranked fit for a free, BYOK, Windows voice assistant
1. **Offline HAR import from the user's own session** (mitm2openapi-style, redaction on by default) — no proxy, no CA trust.
2. User-driven capture in the user's real browser (reverse-api-engineer's keep-the-session mode).
3. **LLM at analysis time only** (spectral model: the LLM never runs at call time) — fits free-tier keys; Groq/Gemini compatibility unverified.
4. Local MITM capture for non-browser clients — highest friction/risk (captures contain secrets).
5. UIA tree as *context* (window/URL tagging), not as an endpoint source.
Guardrail to adopt: apisniff's authorization sentence; none of the other READMEs state ToS/authorization guidance.
