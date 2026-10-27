# Activity Manager icons

The 90 SVGs here are bundled locally. Activity Manager makes no icon-service
request at runtime and needs no Logo.dev account or monthly request allowance.
They cover the recognized desktop brands and common web apps found in the
Windows Start menu inventory on 2026-10-21. `genericapp.svg` is a local NEXUS
fallback for an unrecognized app; it does not impersonate a brand. Start menu
entries include Windows tools and browser shortcuts, so 218 entries do not
imply 218 separately identifiable foreground applications.

| Source | Assets | License record |
| --- | ---: | --- |
| `@iconify-json/logos` 1.2.17, [SVG Logos](https://github.com/gilbarbara/logos) | 22 | CC0-1.0 |
| `@iconify-json/vscode-icons` 1.2.92, [VSCode Icons](https://github.com/vscode-icons/vscode-icons) | 5 | MIT; `LICENSE-vscode-icons.txt` |
| [theSVG](https://thesvg.org), via `@thesvg/mcp-server` 0.8.4 | 62, including VLC | Per-icon source slug and license in `scripts/activity-icon-sources.json`; VLC is CC0-1.0 |
| NEXUS local vector | 1 | `genericapp.svg` |

The curated theSVG import uses only entries marked CC0-1.0, MIT, or Apache-2.0.
`scripts/activity-icon-sources.json` pins its package version, slug, variant,
and reported license for each of 61 icons. The VLC asset was separately
retrieved with the same MCP (`vlc-media-player`, `default`). Brand colours for
the new icons are stored in `frontend/src/settings-sidebar/hub/activityIconColors.json`.

To refresh the Iconify assets, obtain the exact package versions, extract their
`package/icons.json` files, then run
`node scripts/vendor-activity-icons.mjs <logos/icons.json> <vscode-icons/icons.json>`
from the repository root. To fetch any *missing* curated theSVG icons, run
`node scripts/import-activity-icons-mcp.mjs`. The importer does not overwrite
existing assets. Review upstream marks and license data before replacing them.

Product names and marks belong to their owners; icon presence does not imply
affiliation. Browser sites remain included in browser app time; an optional
extension can also report their domains as a separate breakdown.
