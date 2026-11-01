# Place card sidebar - implementation plan (for review, nothing built yet)

Research: `reports/NEXUS place card data sources.md` (notes in `research_notes/NEXUS place card data sources/`).

## What the user gets
Say "where is Charminar" (or any shop, mall, restaurant): the map panel opens as today, and a narrow **place card** opens beside it, 315 logical px wide, height fitted to the screen:

1. Photo (one), with attribution when the source requires it
2. Name + category
3. **Distance from you** in km (road distance)
4. **Car** time and **bike** time (two chips)
5. **Address**
6. **Fare estimate** block: auto / cab / bike-taxi, a range, labelled "Estimate, not a live price"
7. Hours today, phone, rating (only when a source has them)
8. Action row: Directions (the map), Open in Uber / Ola / Rapido (app links), Call, Copy address

## What is possible, what is not (from the research)
| Item | Verdict |
|---|---|
| Distance, car time | Yes. Google Routes with the user's key; else OpenRouteService (free key); else public OSRM; else straight line x 1.3 (marked "about"). |
| Bike time | Google TWO_WHEELER (beta, India coverage not confirmed). Without a Google key: car time scaled by a labelled assumption, "about". No free hosted motorbike profile was verified. |
| Address | Google `formattedAddress`, else Nominatim (1 req/s, cached), else Photon. |
| Photo | Google Places photo (needs the user's key), else Wikipedia/Commons (landmarks only), else Mapillary (street view only), else a map image labelled as such. Shops/restaurants without a Google key will often have no real photo. |
| Hours / phone / rating | Google fields (1,000 free a month), else OpenStreetMap tags (sparse in India), else omitted. |
| **Live Uber / Ola / Rapido prices** | **Not available legally.** Uber's price API needs approval and its terms bar competitor comparison. Ola's estimate API is reported invite-only. Rapido has no official API. Scraping or reverse-engineered endpoints risk terms violations and account bans, so we will not use them. |
| Fare estimates | Yes, as a clearly labelled estimate: Telangana auto tariff (sources disagree on the slab, see open questions), wide ranges for cab and bike taxi, editable in settings. |
| Open in Uber | Documented deep link. Ola and Rapido link formats must be verified first. |

## Window and sizing
- New pinned view `place` (`sidebar.html?pin=place`) handled by `panel_layout` like the Maps panel; label `place-card`.
- Width = 315 x scale factor. Height = about 92 % of the work area, clamped to 480-1100 logical px, so it fits 1366x768 at 125 % up to 2560x1600. Below about 560 px the photo and hours collapse and the body scrolls.
- Placed next to the map panel by the layout manager; Escape closes it (same native watcher); no title bar, no dock buttons.

## Build steps
1. `place_card.rs`: pure model (`PlaceCard` with optional fields), source chain per field, 10-minute cache keyed by place ID, every field optional so a missing source never blocks the card. Unit tests with fixtures for each parser.
2. Sources: Google Places (details + photo, fetched on demand, photo names never cached), Routes (DRIVE + TWO_WHEELER), ORS/OSRM/Nominatim clients with a real User-Agent and rate limits, Wikipedia summary image, Mapillary token optional.
3. `fares.rs`: pure fare formula from distance and time, tariff table in a JSON file the user can edit, output always a range plus the label.
4. Orchestrator: `run_place_info` builds the card, shows the map and the card together, speaks one line (distance and car time). Directions-from-here already works.
5. Frontend `PlaceCard.tsx`: skeleton while loading, per-field "unavailable" states, light/dark tokens, 24 px minimum touch targets, `aria-live` for updates.
6. Settings: Google key (existing), optional ORS and Mapillary keys, fare table editor, "show fares" switch.
7. Verify: unit tests, CI gates, then a live run on 30-50 real Hyderabad places to measure photo and address hit rates (replaces the estimates in the research).

## Decisions
1. **Photos without a Google key: DECIDED (user, 2026-11-01)** - show a map image (labelled as a map, never passed off as a photo of the place) for shops and restaurants; no key is required.

Still open (recommended default in brackets):
2. **Fare block**: ship the labelled estimate now, or hide fares until a legitimate source exists? [ship the labelled estimate]
3. **Telangana auto slab**: two sources disagree (Rs 30 for 1.6 km + Rs 14/km, or Rs 30 for 1.9 km + Rs 15/km). [I check the transport department notice first]
4. **Bike time without Google**: show "about" from the car time, or hide bike time? [show "about"]
5. **Location**: the card uses the same current-location source as directions (Windows location, rough network fallback). Fine? [yes]

## Later (not in this plan)
ONDC / Namma Yatri real quotes (needs buyer-app onboarding), Uber partner access, booking.
