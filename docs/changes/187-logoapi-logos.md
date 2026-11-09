# 187 - Logos fetched through the logoapi.sh MCP, bundled offline

- `scripts/import-logoapi-icons.mjs` + `scripts/logoapi-icon-sources.json`: dry run by default; checks the brand name logoapi
  reports, SVG safety, size, duplicate hashes, parent-company domains and aspect ratio; `--apply` writes the files and `SOURCES.json`.
- Result: 21 square marks replaced or added (anydesk, blender, brave, copilot, firefox, gmail, googlecloud, googlecolab,
  googledrive, googlephotos, jules, render, rockstar, signal, skype, swiggy (new), trae, twitter (now the X mark),
  visualstudio, vscode, windsurf). About 40 more are wordmark lockups in logoapi and keep their existing square mark;
  googlemeet and kilo were rejected by eye; the Microsoft products, ChatGPT/Codex, Chrome and Android Studio have no correct
  logoapi match. Reasons are in the manifest `keep` list.
- New gate `scripts/check-activity-icons.mjs` (ci-local + ci.yml): provenance + hash, no external references, every UI id has a file.
- Wiring: kilo/devin/trae alerts now get logos (`notify_toast.rs::icon_id`, `KNOWN_ICONS`); Windsurf now has a dark mark so its tiles are light.
- Offline: nothing fetches an icon at run time; files are embedded in the exe.
- Not changed: the MCP panel's inline icons (already offline), `brand_logo.rs` (separate opt-in).
