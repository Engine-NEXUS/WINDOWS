# Place data mediator: Supabase + Cloudflare + home server (plan, nothing built)

Follows 154 (place card) and 155 (design and photo sources).

## The idea as asked
Run Puppeteer over Google Maps for all of India, collect image links, store them in the owner's Supabase database, and make Supabase the mediator between Cloudflare, the users and a home server, with access limited to people the owner approves.

## What I will not plan: the Google Maps scrape
- Scraping Google Maps at India scale breaks Google's Maps Platform terms (the "No Scraping" clause) and the photos belong to the people who uploaded them. Building a database of those links and serving it to other people adds a second problem on top of the first: NEXUS would be redistributing Google content, with the owner's name and server as the target of any complaint or ban.
- It also does not work well: Maps blocks bots, India has tens of millions of places, and a crawl of that size needs rotating proxies and CAPTCHA evasion, which is the part sources rate as the riskiest.
- Even Google's own paid API forbids this: place photo names expire and must not be stored; coordinates may be cached for only a limited time.
So the plan below keeps your architecture (Supabase as mediator, Cloudflare in front, a home server doing the heavy work, an approved-people-only list) and replaces the scrape with sources we are allowed to store.

## Architecture
```
NEXUS app --HTTPS--> Cloudflare Worker (edge: auth check, rate limit, cache)
                         |
                         v
                     Supabase (mediator)
                       - Postgres + RLS: places, images, users, access log
                       - Auth: invited users only (no public sign-up)
                       - Edge Functions: thin glue
                         |
                         v  (Cloudflare Tunnel, no open ports at home)
                     Home server (heavy jobs)
                       - routing: OSRM/Valhalla for India (car + motor scooter)
                       - geocoding: Nominatim/Photon for India
                       - ingest jobs: Wikidata/Commons, OSM, Overture/Foursquare OS Places
                       - optional: website preview-image fetcher
```
Each layer does one job: Cloudflare keeps strangers out and absorbs load, Supabase owns identity and data, the home server owns compute.

## What the home server is genuinely good for (this is the real win)
1. **Routing with a motorbike profile for India**: self-host Valhalla (has a `motor_scooter` costing) or OSRM built from the OpenStreetMap India extract. This fixes the weakest part of the card today (bike time is a guess) with no per-request cost and no rate limits. Traffic-aware times still need Google; self-hosted gives free-flow times, labelled as such.
2. **Geocoding and addresses** from the same OSM extract (Nominatim or Photon): no 1 request/second public limit.
3. **A place image index built only from storable sources:**
   - Wikidata P18 and Wikimedia Commons (landmarks, monuments, temples, stations, colleges): bulk and by geosearch, with licence and author per file.
   - Wiki Loves Monuments India (a large Commons collection of Indian heritage sites).
   - OSM tags `image`, `wikimedia_commons`, `wikidata`, `mapillary` where present.
   - Business websites' own preview image (`og:image`) fetched politely (robots.txt respected, one request per site, cached) for places whose website we know from OSM/Overture/Foursquare OS Places (open, Apache 2.0).
   - Store: image URL, source, licence, author, fetch date, perceptual hash. Do not store Google content.
4. **Overture Maps / Foursquare OS Places** as the open place list (names, categories, coordinates, websites) so "what is this shop" has an answer without Google.

## Supabase schema (first cut)
- `places(id, name, category, lat, lon, address, website, osm_id, wikidata_id, source)`: indexed by geohash and a trigram name index.
- `place_images(place_id, url, source, licence, author, attribution, phash, verified, fetched_at)`.
- `profiles(user_id, role, approved_by, approved_at, revoked_at)` and `access_log(user_id, action, at)` without coordinates.
- Row-level security: users read `places`/`place_images`; only the service role (home server) writes; only the owner edits `profiles`.

## Access control (limited people)
- Invite-only: Supabase sign-up disabled, owner creates or approves each user; short-lived JWT; the Worker verifies the JWT and checks `profiles.revoked_at` (cached for seconds) so revoking someone takes effect almost immediately.
- Home server reachable only through a Cloudflare Tunnel plus Cloudflare Access (service token), no inbound ports, SSH off the internet.
- Per-user rate limits at the Worker; request signing between Worker and home server; secrets in Worker/Supabase secrets, never in the app.
- Home server hardening: separate unprivileged user, firewall deny-all inbound, automatic updates, backups of the database, a kill switch (Worker flag) that turns the whole thing off.

## Privacy
- The card needs the user's location only to route. Best: route on the home server and send coordinates over TLS without logging them; keep `access_log` free of coordinates; document it in the app.
- Never store a user's home or live location in Supabase.

## Fit with what exists
- NEXUS stays free and BYOK: the app keeps working with no server (current chain). The mediator is an optional upgrade for invited users, so it must fail open to the local chain.
- It builds on the Worker already deployed (`server/worker`), the existing device-link/token work, and the place card chain.

## Phases
1. Home server: build OSRM/Valhalla and Nominatim for India from OSM; measure bike/car time against Google on 30 real routes.
2. Supabase: schema, RLS, invite flow, access log; Worker verifies JWT and proxies to the tunnel.
3. Ingest jobs: Wikidata/Commons index for India, then OSM/Overture places, then website preview-image fetch with robots.txt and rate limits.
4. NEXUS client: optional "NEXUS server" setting (URL + invite token); place card asks the server first, falls back to the local chain.
5. Measurement run: photo hit rate, address, bike-time error, and cost; then decide on a vision check for image relevance.

## Open decisions
1. Home server hardware (RAM/disk) and whether it is online 24/7.
2. Who gets access first, and how invites are issued.
3. OK to store only licensed/open image links (no Google content)?
4. OK to fetch website preview images with robots.txt respected?
