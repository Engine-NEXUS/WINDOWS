# Place card: visual design and photo sources (plan, nothing built)

Follows `154-place-card-sidebar-plan.md`. Part A is the design. Part B is the research on getting an image for any location, including the "scrape image links with Puppeteer" idea.

---

## Part B first: can we scrape image links for every place?

### Short answer
Not from Google, and a Puppeteer scraper is the wrong tool even for the sources where it would be allowed. There is a better set of legal sources and one cheap trick (the website's own preview image) that covers shops and restaurants without scraping.

### Why not scrape Google (Maps / Images)
- Google Maps Platform terms have a "No Scraping" clause (section 3.2.3(a), cited by several secondary sources). Driving a headless browser over Maps/Images is exactly what it forbids. Photos are also copyrighted by the people who uploaded them, so there is a second exposure on top of the terms.
- Enforcement is real in practice: bot defences, CAPTCHAs and IP bans. Bypassing them raises the risk further (sources rate bypassing as the riskiest part). Most "is it legal" sources are SEO blogs, so treat them as secondary, and none of this is legal advice.
- A free product shipped to everyone is a worse place to take that risk than a personal script: one takedown or ban hits every user.

### Why not Puppeteer even for allowed sources
- Needs Node plus a bundled Chromium (about 150-300 MB) on top of our 600-700 MB RAM ceiling and installer size.
- Slow (seconds per page), flaky when pages change, and needs maintenance forever.
- Everything legal below is a plain HTTP/JSON call that Rust already does in milliseconds.

### What I tested myself (real calls, Hyderabad, today)
| Source | Landmark (Charminar) | Ordinary restaurant (Paradise, Secunderabad) |
|---|---|---|
| Wikimedia Commons geosearch (files within 300 m, no key) | 5 files (e.g. "Charminar night view.jpg") | 0 usable results (request errored or empty; needs a retest) |
| Wikidata entity search | exists | no entity |
| Openverse (CC-licensed, aggregates Flickr etc.) | request failed once, retest needed | 12 results, but irrelevant ("Paradise in the dark") and several are non-commercial/no-derivatives licences |
| Overpass (OSM tags `image`, `wikimedia_commons`) | not reached (server returned nothing) | not reached |
Reading: open sources are excellent for landmarks and useless for ordinary businesses. Text search for a business name returns noise, so any candidate needs verification before we show it.

### The legal chain I recommend (replaces "scrape multiple sources")
1. **Google Places photo** with the user's own key (already built). Best for businesses. Terms: fetch on demand, never cache photo names, show author credit.
2. **Wikidata / Commons by coordinates** for landmarks: `wikibase_item` from the Wikipedia summary, then P18; else Commons geosearch (files within ~300 m). Keyless, licences known per file (show credit).
3. **The place's own website preview image** (new idea): Google Places returns `websiteUri`; OSM has a `website` tag; Wikidata has P856. Fetch that page with a normal HTTP GET and read `<meta property="og:image">`. Businesses publish that image precisely so apps can show it as a link preview (Slack, WhatsApp and Twitter do the same). One request, no browser, covers many shops/restaurants/malls that have a site. Caveats: copyright stays with the business (show it small, credit "from their website", do not store), some sites block bots, JS-only sites have no tag.
4. **Mapillary** street-level photo near the coordinates (free token, CC BY-SA, attribution). Shows the street, not the shop, so label it "street view".
5. **Map image** (current fallback, labelled).
6. **Verification step** (optional, uses the Gemini key NEXUS already has): send the candidate image plus the place name to the vision model, "does this image plausibly show X? yes/no", and reject noise. Cheap, and it makes Openverse/Flickr usable for businesses. Image-text matching models of this kind (CLIP-style) are the standard approach in the literature; I did not find a paper on "representative photo per place" specifically, so this part is my design, not a cited result.

### Research and existing projects (what exists)
- **Google Landmarks Dataset v2** (CVPR 2020): 5M+ images, 200k landmarks, images taken from Wikimedia Commons because it is the largest crowd-sourced landmark photo collection. Supports using Commons as the primary landmark source. It does not cover shops.
- **OpenStreetMap "Photo linking" wiki**: documents tags for attaching photos (`image`, `wikimedia_commons`, `wikidata`, `mapillary`, `panoramax`). Coverage in India is unmeasured; likely sparse for businesses.
- **Mapillary** open tooling (MapillaryJS, Pic4Review) and an archived "OSM POI viewer with Mapillary" project: pairs POIs with street imagery. Archived 2022, so a pattern to copy, not code to reuse.
- **Foursquare Open Source Places** (Apache 2.0, 100M+ POIs): open data, but I found no photo field in the open release. Photos are in paid products.
- **Openverse**: CC-licensed aggregator (no key anonymously, 20 per page). It states it does not verify licences, so each result needs its licence and credit checked.
- I found no maintained open-source project that returns one good photo for any business without a commercial API. That gap is why the paid APIs exist.

### Decision requested
Build chain items 1-3 and 5 now; add 4 and 6 later if measured hit rates are low. Do not build a Puppeteer scraper. Before shipping, run a measurement on 30-50 real Hyderabad places to replace the guesses in this document.

---

## Part A: design (designer's view)

### Principles
1. One glance answers "how far, how long, what is it". Photo and the distance number carry the card; everything else is secondary.
2. Honest by design: estimates are visibly estimates, a map image is visibly a map, missing data shows nothing instead of a blank row.
3. Works at 315 px and on short laptop screens: the middle scrolls, the header and the action bar never do.
4. Calm: no motion beyond a skeleton and a fade; respects reduced motion; same tokens as the rest of NEXUS in dark and light.

### Layout (315 px wide, 8 px grid, 16 px side padding)
```
+-------------------------------+
| [ photo 16:9 , soft scrim  ]  |  <- 177 px tall, collapses to 96 px under 620 px screen height
|  (x)                  1/3 o   |     close button top-right; credit chip bottom-left
+-------------------------------+
| [utensils] Restaurant         |  <- category pill with icon
| Paradise Biryani              |  <- 20 px / 600, max 2 lines
| ★ 4.3 (rating)  · Open now ·  |  <- status dot: green open, red closed
|   closes 11 pm                |
+-------------------------------+
|  22.6 km  from you            |  <- 28 px / 600 number, 12 px label
| +-----------+ +-------------+ |
| | [car]     | | [bike]      | |  <- two equal chips with icons
| | 48 min    | | ~41 min     | |     "~" and a footnote when estimated
| +-----------+ +-------------+ |
| (i) Estimate, no live traffic |
+-------------------------------+
|  RIDE ESTIMATE                |  <- section label, 11 px caps
| [auto]  Auto       Rs 320-370 |  <- icon + name left, range right, tabular numerals
| [bike]  Bike taxi  Rs 135-210 |
| [cab ]  Cab        Rs 305-485 |
|  Not live prices.  (i)        |  <- (i) opens one line: why we cannot show live ones
+-------------------------------+
| [pin]  Address text...  [copy]|
| [clock] Today 9:00-5:30  v    |  <- chevron expands the week
| [phone] 040 6674 5986  [call] |
|  "Editorial summary, 2 lines" |
+-------------------------------+
| [ Directions ]  [ Uber ]  ... |  <- pinned action bar, 40 px buttons, never scrolls
+-------------------------------+
```

### Visual language
- **Type scale** (system UI font already used): 11 caps labels, 12 secondary, 13 body, 20 title, 28 distance. Tabular numerals for all numbers so ranges line up.
- **Colour**: only existing `--ui-*` tokens (dark and light both pass the project's contrast gate). Meaning colours: open = green, closed = red, estimate note = tertiary text. One accent (the NEXUS accent) for the primary action. No gradients except a 40 % scrim at the photo's bottom for the credit chip.
- **Shape**: 12 px card radius, 10 px chips, 8 px buttons, 1 px subtle borders, no shadows inside the card.
- **Icons**: one consistent line-icon set drawn as inline SVG (24 px grid, 1.75 px stroke, `currentColor`), taken from Lucide (ISC licence): car, bike/motorbike, map-pin, clock, phone, copy, utensils, shopping-bag, store, landmark, hospital, fuel, pill, graduation-cap, hotel, info, x. A small category-to-icon map picks the pill icon from the place type.
- **Logos**: tiny monochrome brand marks only on the two link buttons ("Google Maps" and "Uber"), from Simple Icons (CC0 files; the brands' trademark rules still apply, so used only to label a link to that service and never to imply endorsement). Not used for Ola and Rapido until their link formats are verified.
- **Photo treatment**: crossfade from skeleton, 16:9 cover crop, credit chip bottom-left, a "Map" chip when it is a map image. Tap to enlarge is a later step.
- **States**: skeleton shimmer for photo/trip/rows while loading; each row has its own "unavailable" line instead of vanishing silently only when the source failed (rows with no data at all stay hidden); offline shows cached card with "offline" chip; error never shows raw text.
- **Accessibility**: 24 px minimum targets (40 px for buttons), focus rings via tokens, headings in order, `aria-live` on the trip block, every icon either labelled or `aria-hidden`, reduced-motion disables shimmer and fades.

### Responsive behaviour
- Width fixed at 315 CSS px; height about 92 % of the work area, clamped 480-1100.
- Under about 620 px: photo shrinks to 96 px, hours and summary collapse behind chevrons, fares stay.
- Under about 520 px: photo hidden, name and distance move to the top, body scrolls.
- 125/150 % scaling is handled by sizing the window in physical pixels.

### What changes in the build
1. New small icon module (`placeIcons.tsx`) with the inline SVGs and the category map; unit test for the map.
2. Restyle `PlaceCard.tsx` to the layout above: category pill, status row, trip block with chip icons, icon rows with copy/call actions, expandable hours, sticky action bar with the two brand marks.
3. Skeleton and per-row states; responsive collapse rules in `unified.css`.
4. Photo chain items 2 and 3 in `place_card.rs` (Wikidata/Commons by coordinates; website `og:image` fetch with size limit, redirects limited, https only, data-URI conversion as today).
5. Tests: category-to-icon map, og:image parser (pure), Commons geosearch parser (pure), CSS contrast gate.
6. A 30-50 place measurement run (photo hit rate, address, bike time) and then tune.

### Open decisions
- OK to use the website preview image (`og:image`) for shops and restaurants, shown small with "from their website"?
- OK to add the optional vision-model check (uses the user's Gemini key, one small request per card)?
- Which brand marks: Google Maps and Uber only, as proposed?
