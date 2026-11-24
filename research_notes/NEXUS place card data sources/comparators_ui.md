# Ride-fare comparators and compact place-card side panels (as of Nov 2026)

## 1. Open-source ride-fare comparison projects: how do they get prices, and do they work?

### Takeaway
I found no maintained, permissively licensed open-source Uber/Ola/Rapido fare comparator. What exists is one academic prototype with no code link, commercial scraping vendors, a closed consumer app, and an ONDC/Beckn route that is open but covers only participating providers (for example Namma Yatri). Uber, Ola and Rapido fares have no public API for third parties in India.

### Cited Findings
- arXiv 2512.04065 (Dec 2025) describes a web app comparing Ola, Uber and Rapido, with a Python backend. The abstract mentions API access difficulties, the Android Studio emulator and Appium. No repository link is given on the page. — [arXiv abstract](https://arxiv.org/abs/2512.04065v1)
- Per third-party summaries of that paper, Ola fares were obtained by mirroring Ola's public pricing model, Uber fares were randomly generated test data because API access was restricted, and Rapido used Bangalore empirical data. Tested only in an emulator on simulated routes. Summaries are secondary sources. — [Moonlight review](https://www.themoonlight.io/en/review/fare-comparison-app-of-uber-ola-and-rapido), [Emergent Mind](https://www.emergentmind.com/papers/2512.04065)
- Commercial vendors (Actowiz) sell scraped Uber/Ola/Rapido fare data via a REST API, with fares, categories and ETAs. These are marketing pages; their claims are unverified, and none address ToS legality. — [Actowiz](https://www.actowizsolutions.com/uber-ola-rapido-fare-comparison-data-scraping.php)
- Comparoo (Product Hunt) claims to compare Uber, Ola, Rapido and Namma Yatri prices for the same ride type. It is not described as open source and the "cheaper by Rs 80" claim has no methodology (unverified). — [Product Hunt](https://www.producthunt.com/p/comparoo)
- Namma Yatri is described as open source, built on the Beckn protocol and integrated with ONDC; drivers pay a subscription rather than a commission (2023-2024 reporting, may be dated). — [MediaNama](https://www.medianama.com/2023/03/223-namma-yatri-ride-hailing-app-ondc/), [Business Today](https://businesstoday.in/technology/news/story/google-invests-in-indian-open-source-app-that-rivals-ola-and-uber-report-437437-2024-07-17)
- ONDC-Official on GitHub hosts mobility specs (on-demand ride hailing TRV10 v2.0.1), with sandbox and mock server resources. Buyer apps must register as network participants. — [ONDC-Official](https://gh.hhj520.top/ONDC-Official) (mirror domain as returned by search)
- dumko2001/mcp-beckn-mobility: a community MCP "buyer app" bridging AI agents to Beckn mobility providers (Namma Yatri/ONDC) with Ed25519 signing; the listing shows 0 stars, no licence listed, no update date. Treat as a toy until the repo is inspected. — [MCP Market listing](https://mcpmarket.com/es/server/beckn-mobility)
- Method summary across sources: official API (Uber US only, see section 2), formula mirroring (Ola, paper), UI automation via emulator + Appium (paper), commercial scraping (vendors), open network (Beckn/ONDC).

### Inferences
- For NEXUS, a trustworthy fare for Uber/Ola/Rapido in India is not obtainable from open components. Honest options: (a) show car/bike distance and time only, (b) show a clearly labelled "estimate" computed from a user-editable per-km formula, (c) deep-link into each app's booking screen, (d) query ONDC/Beckn mobility for real quotes from participating providers (requires network participation or a gateway; effort unknown).
- Scraping or accessibility-automating third-party apps carries ToS and fragility risk; I did not find sources on legality.

### Gaps
- No GitHub repo for any working Uber/Ola/Rapido comparator was found; GitHub search itself was not run directly (only web search), so a small project could be missed.
- Rapido/Ola public APIs or deep-link URL schemes were not researched.
- Current ONDC buyer-app availability and live fares unconfirmed.

## 2. MCP servers for maps, places, routes, ride estimates

### Takeaway
Google now offers an official remote MCP (formerly Maps Grounding Lite, now "Maps Experience agent tools") but it is billed, needs a key, and has display/attribution rules. OSM/OSRM community MCP servers are MIT and keyless but small and unvetted. Uber ride estimates are official only for US per search summary; community Uber servers are stale.

### Cited Findings
- Google official: server URL `https://mapstools.googleapis.com/mcp`, Streamable HTTP; tools search places, lookup weather, compute routes (distance/duration only, no steps), resolve names / Maps URLs. Auth by `X-Goog-Api-Key` or OAuth; a demo key exists for testing. — [Google docs](https://developers.google.com/maps/ai/grounding-lite)
- Pricing: Essentials $7 per 1,000 requests with 10,000 free per month; Pro $14 per 1,000 with 5,000 free, Pro in restricted GA. Quota 300 queries/min per project. — [Google docs](https://developers.google.com/maps/ai/grounding-lite)
- Terms: sources must be attributed to Google Maps and linked via `placeUrl`, viewable within one interaction; voice interfaces need a companion UI showing sources; the LLM must not train on input data. Regions not stated on that page. — [Google docs](https://developers.google.com/maps/ai/grounding-lite)
- Tool-count discrepancy: Google's reference examples show three tools; a third-party directory says five. — [Google MCP reference](https://developers.google.com/maps/ai/grounding-lite/reference/mcp), [directory](https://www.developersdigest.tech/mcp-servers/google-maps-grounding-lite)
- Community Google Maps MCPs: artixz/mcp-google-maps (MIT per search summary; directions, distance matrix, elevation, static map), arthurkatcher/google-maps-mcp (Python), ettysekhon/google-maps-mcp-server, cablate/mcp-google-map; maintenance not verified. All need a Google API key. — [search results](https://github.com/artixz/mcp-google-maps), [lobehub](https://lobehub.com/mcp/ettysekhon-google-maps-mcp-server)
- GRABOSM/osm-mcp: MIT, 30 tools (geocoding, POIs, OSRM routing, distance matrix, isochrone, Overpass); runs stdio (Node 18+) or as a local HTTP REST server on port 8888; auth/stars/last update not shown. — [Glama](https://glama.ai/mcp/servers/@GRABOSM/osm-mcp)
- dodbrian/osmaps-mcp: TypeScript, OSRM + Nominatim, no API key, 0 stars, last push April 2026 (per search summary). NERVsystems/osmmcp is a Go implementation. — [ecosyste.ms](https://awesome.ecosyste.ms/projects/github.com%2Fdodbrian%2Fosmaps-mcp), [mirror](https://gh.r-mj.com/NERVsystems/osmmcp)
- The public OSRM demo server serves car routing only; real bike/foot routing needs a self-hosted OSRM (per myosm-mcp README summary). — [Glama listing](https://glama.ai/mcp/servers/hrk9zvug1i)
- Uber: search reports an official remote MCP at `mcp.uber.com/claude/rides-3p/mcp` with OAuth, US trips only (single secondary summary; not verified by me). — [Glama Uber integrations](https://glama.ai/mcp/servers/integrations/uber)
- Community Uber MCPs: 199-mcp/mcp-uber (ISC, calls Uber /v1.2/estimates/price, last updated 2025-06-19, grade D); Strider Labs @striderlabs/mcp-uber uses browser automation. Most "Uber" MCP results are SMS-OTP or unrelated. — [Glama](https://glama.ai/mcp/servers/integrations/uber), [mcpservers.org](https://mcpservers.org/id/servers/199-biotechnologies/mcp-uber)
- Official MCP registry is not an authority on quality; no official modelcontextprotocol-org maps server surfaced in search. — [search](https://github.com/artixz/mcp-google-maps)

### Inferences
- Inside a Tauri/Rust app, HTTP-transport MCP (Google's) is easiest to call directly with reqwest; stdio servers need a Node runtime bundled or spawned, which conflicts with the app's RAM and no-extra-runtime goals. NEXUS already has `mcp_client.rs` speaking streamable HTTP.
- For a free BYOK product, calling OSRM/Nominatim/Overpass directly in Rust (or Google Routes/Places with the user's key, as NEXUS already does) is lighter than adding MCP servers; the MCP servers are mainly useful as API-shape references.
- Nominatim and OSRM public instances have usage policies (not researched here); heavy use needs self-hosting or a provider.

### Gaps
- Licences, last-commit dates and stars for the Google community MCPs were not verified.
- No India coverage statement was found for Google's MCP.
- Official Uber MCP existence and terms need confirmation from Uber's developer site.

## 3. Place-card UI patterns for a ~315 px column

### Takeaway
Apple's documented place card order is header image, then name/category/rating, then hours, then contact (website, phone, address), then an "open in Maps" link. Google Maps and Raycast-style card details were not retrieved, so the order below is partly my inference.

### Cited Findings
- Apple HIG: the full callout place card has header image with place name, category and rating at top; tiles for operating hours; website/phone/address; and an "Open in Apple Maps" link. Presented as popover on iPadOS/macOS and sheet on iOS; a minimum width can be set to avoid text overflow. — [Apple HIG Maps](https://developer.apple.com/design/human-interface-guidelines/maps.md), [Apple Business place card](https://business.apple.com/docs/onboarding-guide/place-card)
- Apple advises keeping the location visible on the map while a place card is shown. — [Apple HIG Maps](https://developer.apple.com/design/human-interface-guidelines/maps.md)
- Material 3 side sheets are anchored to an edge, fixed width, typically full height; modal on small screens, standard on tablet/desktop; body keeps a 48dp trailing margin. Width band 256-400dp comes from secondary sources (a Rust port uses 320dp default) and is unverified against m3.material.io. — [Material side sheets](https://material.io/components/sheets-side), [frust-material](https://docs.rs/crate/frust-material/0.5.2/source/src/side_sheet.rs)

### Inferences
- Suggested 315 px order: photo (16:9, about 175 px high, collapsible when short), name plus category line, rating and open/closed status, two chips (car time/distance, bike time/distance, plus fare estimate only if sourced), address, hours (collapsed to today), then actions (Directions, Open in Maps, Call, Copy). This mirrors Apple's order with travel chips promoted because that is the product's differentiator.
- 315 px is within the 256-400 band, so a fixed-width side sheet with height flexing is consistent with Material guidance.
- Under a short screen (768 px high), drop to: name, category, chips, address, actions; make photo and hours collapsible; scroll the body, pin the action row.

### Gaps
- Google Maps place sheet, Raycast/Alfred/Spotlight rich results and macOS Look Around specifics were not found in sources this pass.

## 4. Responsive sizing, DPI and Tauri windows

### Takeaway
Tauri exposes each monitor's physical size, position, work area (physical rect) and scale factor, so a panel can be sized as a percentage of the work area in physical pixels then clamped. Official best-practice text was not found; source-level facts only.

### Cited Findings
- Tauri's monitor struct has size, position, work_area (`PhysicalRect`) and scale_factor; convert between logical and physical with the scale factor. — [tauri-runtime monitor.rs](https://docs.rs/tauri-runtime/latest/src/tauri_runtime/monitor.rs.html)
- tao enables per-monitor DPI awareness (V2, falling back to V1); scale factor = DPI / 96; DPI-unaware processes report 1.0. — [tao dpi.rs](https://docs.rs/crate/tao/latest/source/src/platform_impl/windows/dpi.rs)
- Existing NEXUS notes record the sidebar height already uses monitor height minus margins, clamped (for example `(monitor_h - 40).min(1040).max(400)`) with a 12 px right offset; that is the in-repo precedent (from AGENTS.md).

### Inferences
- Do the maths in physical pixels from `work_area` (excludes the taskbar), e.g. height = clamp(0.92 x work_area.h, 480 x scale, 1100 x scale); width = 315 x scale_factor so CSS stays 315 px at 100/125/150/200 percent. At 1366x768 (125 percent, about 1093x614 logical) the card must work at about 560 px of logical height, so design for a 480 px minimum.
- Recompute on monitor change or scale-factor-changed events; mixed-DPI setups were not researched.

### Gaps
- No official Tauri guidance on work-area sizing found; behaviour on multi-monitor mixed DPI not verified.

## 5. Accessibility, theming, loading and failure states

### Takeaway
Use WCAG 2.2 AA targets: 24x24 CSS px minimum target size and 3:1 non-text contrast, including focus indicators. Theming and state patterns below are my recommendations, not sourced.

### Cited Findings
- WCAG 2.2 SC 2.5.8 (AA): pointer targets at least 24x24 CSS px with exceptions; 44 px is a recommendation (AAA 2.5.5), not required at AA. — [DEV summary](https://dev.to/thoithoi_shougrakpam_6753/wcag-22-what-changed-why-it-matters-and-how-to-implement-it-98e), [getwcag](https://getwcag.com/blog/wcag-2-2-checklist)
- SC 1.4.11: 3:1 contrast for UI components, graphical objects and focus indicators against adjacent colours. — [Stark](https://getstark.co/wcag-explained/perceivable/distinguishable/non-text-contrast)
- Google's terms require visible attribution/link when Google Maps data is shown (section 2). — [Google docs](https://developers.google.com/maps/ai/grounding-lite)

### Inferences
- Theming: define colour tokens on `:root`, override under `prefers-color-scheme: dark` plus an explicit theme attribute (NEXUS already syncs a light/dark setting across windows); photos get a neutral overlay for text legibility.
- States: skeleton for photo and chips while loading; missing photo falls back to a category icon on a tinted block; offline shows last cached card with a "Offline, showing saved details" line; no fare data shows "Fare unavailable" with a deep link rather than a blank or a made-up number; honour `prefers-reduced-motion` for shimmer; announce changes through an `aria-live="polite"` region; real buttons with visible focus rings and Escape to close (matches the recent Escape-closes-panel commit).

### Gaps
- No sources found on skeleton loading, offline UI, `prefers-color-scheme` or reduced-motion practice; the items above are unsourced recommendations.
