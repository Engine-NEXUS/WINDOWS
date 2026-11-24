# Routing, distance, address and place details for the NEXUS place card (as of Nov 2026)

## Google Routes API: TWO_WHEELER, DRIVE, BICYCLE, quota, key needs, Distance Matrix status

### Takeaway
Google's computeRoutes has a real TWO_WHEELER mode (motorized two-wheelers, separate from BICYCLE) with an India (Kolkata) example, but it is beta, requires showing a warning to the user, and bills at a higher rate than other modes. Compute Routes has a free monthly cap per tier (Essentials 10k, Pro 5k, Enterprise 1k), so it suits a BYOK user with their own key.

### Cited Findings
- TWO_WHEELER is for motorized two-wheelers such as motorcycles, distinct from human-powered bicycle mode; set via `travelMode` in computeRoutes; docs example is a Kolkata, India request. — [Google: two-wheeled route](https://developers.google.com/maps/documentation/routes/route_two_wheel)
- Two-wheel routing is beta, "might not always include optimal paths for motorcycles", and Google requires you to display a warning to the user. — [same](https://developers.google.com/maps/documentation/routes/route_two_wheel)
- TWO_WHEELER requests are billed at a higher rate than other travel modes; the page names no SKU. — [same](https://developers.google.com/maps/documentation/routes/route_two_wheel)
- Supported regions are on a separate "countries and regions supported for two-wheeled vehicles" page (my guessed URL returned 404; not read). — [same](https://developers.google.com/maps/documentation/routes/route_two_wheel)
- A response field mask is mandatory (`X-Goog-FieldMask` header or `fields`); omitting it errors; avoid `*` in production. — [Choose fields](https://developers.google.com/maps/documentation/routes/choose_fields)
- Compute Routes free caps per month: Essentials 10,000 ($5.00/1k after), Pro 5,000 ($10.00/1k), Enterprise 1,000 ($15.00/1k). — [GMP pricing](https://developers.google.com/maps/billing-and-pricing/pricing)
- Distance Matrix is listed under "Legacy product pricing"; Compute Route Matrix SKUs under Routes API are the current equivalent (page does not state a replacement explicitly). — [GMP pricing](https://developers.google.com/maps/billing-and-pricing/pricing)
- `TRAFFIC_AWARE` is a `routingPreference` value; the fetched page did not say which tier it triggers. — [Choose fields](https://developers.google.com/maps/documentation/routes/choose_fields)
- A third-party blog claims per-SKU free caps since March 2025 with no pooling across APIs; not confirmed by an official page. — [Woosmap blog](https://www.woosmap.com/blog/google-maps-api-pricing-breakdown)

### Inferences
- Traffic-aware routing is likely Pro tier or above (from memory, unverified).
- Per place card with a Google key: one DRIVE call plus one TWO_WHEELER call, mask `routes.duration,routes.distanceMeters`; about 2 calls per card, so thousands of free cards per month.
- Restrict the key to Routes API (and Places API New). Application restrictions are awkward for a desktop app, so API restriction is the practical option. Billing must be enabled on the project (my inference).

### Gaps
- Whether India is in the supported two-wheeler region list (docs example suggests yes; list page not read).
- TWO_WHEELER SKU tier and price; whether traffic-aware works for TWO_WHEELER.

## Keyless/open routing (OSRM, ORS, Valhalla, GraphHopper, Mapbox, HERE) and motorbike estimation

### Takeaway
No free keyless hosted service was verified to have a motorbike profile; Valhalla has `motor_scooter`/`motorcycle` (self-hosted, or a public instance I did not verify). Best keyless chain: OSRM public (car, no traffic) for road distance plus a labelled estimate for bike time. ORS (free key, 2,000 directions/day) lists car/cycling/foot profiles; GraphHopper free lacks motorcycle/scooter.

### Cited Findings
- OSRM demo hosts: router.project-osrm.org and routing.openstreetmap.de; profiles car, foot, bike; max 1 req/s; "reasonable, non-commercial" use; no guarantees of uptime or latency; FOSSGIS sponsors. — [OSRM Demo server](https://github.com/Project-OSRM/osrm-backend/wiki/Demo-server)
- Policy: valid, non-faked User-Agent, ODbL/OSRM attribution, no iframe embedding of map.project-osrm.org, blocking on excessive use; terms now referenced to the FOSSGIS-run server (that policy not read). — [OSRM API usage policy](https://github.com/Project-OSRM/osrm-backend/wiki/Api-usage-policy)
- OpenRouteService Standard (free): Directions 2,000/day, 40/min; Matrix 500/day; not for commercial use. Profiles named by a third party: driving-car, driving-hgv, cycling-regular, foot-walking. — [ORS plans](https://openrouteservice.org/plans/), [Disy docs](https://docs.disy.net/cadenza/10.5/en/administrator/config-files/openrouteservice.html)
- The ORS API reference page fetched did not list profiles; I found no evidence of a motorbike profile. — [ORS API ref](https://giscience.github.io/openrouteservice/api-reference/endpoints/directions/)
- Valhalla costing: `auto`, `bicycle`, `motor_scooter` (scooters/mopeds; avoids higher-class roads unless the country allows them; use_primary 0.5, use_hills 0.5, top_speed default 45 km/h, range 20-120), `motorcycle` (BETA). Summary returns `time` and `length` (km default). — [Valhalla route API](https://valhalla.github.io/valhalla/api/route/api-reference/)
- GraphHopper free package: non-commercial, limited profiles (car, bike, foot per docs), no flexible mode; motorcycle via Kurviger on request; numeric credits unverified (third parties say 500/day, 5 locations). — [GraphHopper pricing](https://www.graphhopper.com/pricing/)
- Mapbox: sources agree about 100,000 free Directions requests/month (third-party only). HERE free allowance conflicts (30k vs 250k transactions/month). — [Woosmap Mapbox](https://www.woosmap.com/blog/mapbox-pricing), [apiscout](https://apiscout.dev/guides/mapbox-vs-here-maps-api-2026)

### Inferences
- OSRM durations are free-flow from OSM speed profiles with no live traffic (from OSRM design; not stated on pages fetched), so label them "no live traffic".
- Motorbike time from car time is honest only as a labelled estimate (range or "about"). No source validates a car-to-motorbike factor for Indian cities. Never present cycling profile time as motorbike time.
- A public Valhalla instance with motor_scooter may exist (e.g. FOSSGIS); not verified.

### Gaps
- No source comparing OSRM/ORS accuracy vs Google in Indian cities, nor a validated motorbike-to-car time ratio.
- Official current Mapbox/HERE quotas; Valhalla public-instance policy; full ORS profile list.

## Address and place details (Nominatim, Photon, Google Places New, Overpass)

### Takeaway
Nominatim public is fine for occasional reverse lookups with a custom User-Agent, caching, and under 1 req/s. Photon is an alternative with fair-use only. Opening hours, phone and rating are Enterprise-tier fields in Google Places (New) (1,000 free/month); keyless OSM tags are possible but sparse.

### Cited Findings
- Nominatim public: absolute max 1 req/s per app; identifying User-Agent/Referer required ("stock User-Agents ... will not do"); results must be cached; autocomplete and systematic/grid reverse geocoding forbidden; attribution (ODbL) required; regular/long-running scripts capped at 4 req/min. — [OSMF Nominatim policy](https://operations.osmfoundation.org/policies/nominatim/)
- Photon public (photon.komoot.io): free but fair use, heavy use throttled, no availability guarantee; 1 req/s is a client convention; data is OSM/ODbL. — [Photon README](https://github.com/Komoot/photon), [R photon docs](https://www.rdocumentation.org/packages/photon/versions/0.7.4-1/topics/photon-package)
- Google Place Details tiers: `formattedAddress` Essentials; `regularOpeningHours`, `nationalPhoneNumber`, `rating` Enterprise. Field mask mandatory. — [Place Details](https://developers.google.com/maps/documentation/places/web-service/place-details)
- Place Details free caps/month: Essentials 10,000 ($5/1k), Essentials IDs-only unlimited free, Pro 5,000 ($17/1k), Enterprise 1,000 ($20/1k), Enterprise+Atmosphere 1,000 ($25/1k). — [GMP pricing](https://developers.google.com/maps/billing-and-pricing/pricing)
- Overpass main instance: under 10,000 queries/day and 1 GB/day is safe, divide by 100 for regular use; identify via User-Agent; wait 30 s on 429/406; main server noted as overloaded; alternatives include overpass.private.coffee (no rate limit stated) and Overpass.ch (free key, 10k/month). — [OSM wiki Overpass](https://wiki.openstreetmap.org/wiki/Overpass_API)

### Inferences
- With a Google key, request Place Details with mask `formattedAddress,regularOpeningHours,nationalPhoneNumber,rating` only when the card is opened (Enterprise SKU, 1k free/month).
- Keyless: Nominatim reverse (`addressdetails=1`, `extratags=1`) may return opening_hours/phone when OSM has them; Indian POI coverage is unmeasured. Overpass lookup by element id is a secondary option. Show raw opening_hours string if no parser.

### Gaps
- Photon official terms not read; Nominatim extratags coverage in India not measured.

## Haversine fallback and recommended chain

### Takeaway
Compute haversine first (instant, offline), label it straight-line, and replace it with road distance when a router answers.

### Cited Findings
- None; haversine is standard math. The road/straight-line multiplier is unsourced.

### Inferences
Recommended chain:
1. Distance/time: Google key -> Routes DRIVE (traffic-aware optional) + TWO_WHEELER (show Google's beta warning). Else ORS key -> driving-car. Else OSRM public driving (1 req/s, User-Agent, attribution). Else haversine times about 1.3, marked "straight-line approx" (1.3 is a rule of thumb, unverified).
2. Bike time without Google: derive from road km at an assumed urban speed band, shown as "approx., estimate".
3. Address: Google `formattedAddress` if keyed; else Nominatim reverse cached by rounded coordinates; Photon as fallback.
4. Hours/phone/rating: Google Enterprise fields if keyed; else OSM extratags if present; else omit.
5. Labels: "Estimate, no live traffic" on every non-Google duration; "Beta, may not be optimal for motorcycles" on Google TWO_WHEELER; ODbL/OSRM attribution on OSM-derived data.

### Gaps
- Road/straight-line ratio for Indian cities: no source found.
