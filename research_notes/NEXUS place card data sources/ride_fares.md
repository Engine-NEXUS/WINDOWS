# Ride-fare estimates (price only) for Uber, Ola, Rapido and other India apps — Hyderabad, as of Nov 2026

Evidence level: ~12 tool calls, mostly search-result summaries plus 3 page fetches. Items marked [primary] come from the vendor's own docs; everything else is secondary or hearsay.

## Uber: Riders Price Estimates API, deep links, MCP

### Takeaway
GET /v1.2/estimates/price still exists in the docs but needs Uber-approved access via a business contact, and Uber's terms forbid using it for price comparison with competitors. Deep links and the Ride Request Button open the Uber app but do not give you a price in your own UI. An individual developer cannot realistically get a legitimate estimate feed.

### Cited Findings
- [primary] The price-estimates endpoint says Uber must approve access; you contact your Uber business development representative or point of contact. Auth is either a server token or an OAuth user token with any valid scope. — [Uber docs](https://developer.uber.com/docs/riders/references/api/v1.2/estimates-price-get)
- [primary] The same page says using the API to offer price comparisons with competing third-party services violates §II B of the API Terms of Use and may cost you access. It also says Uber is changing its Developer API program with new access policies for third-party apps. It says nothing about India. — [Uber docs](https://developer.uber.com/docs/riders/references/api/v1.2/estimates-price-get)
- No 2026 deprecation of the endpoint was found. An old notice (Dec 2022) applied to POST /requests/estimate, not the price endpoint. — [search summary](https://developer.uber.com/docs/riders/references/api/v1/requests-estimate-post)
- [primary] Deep links (setPickup) can set pickup, dropoff and product_id. A dropoff only displays if a nickname or formatted_address is also given. Deep links are only URL parameters; price estimates require Uber's APIs or the Ride Request Button (product plus pickup shows time, adding dropoff shows price). — [Uber deep-link docs](https://developer.uber.com/docs/deep-linking), [Widget migration](https://developer.uber.com/docs/riders/ride-requests/tutorials/widget/migration-to-deep-links)
- [primary] The old Ride Request Widget is deprecated; Uber's migration options are m.uber.com links, the Ride Request Button (iOS/Android SDK) or SDK-built links. — [Uber docs](https://developer.uber.com/docs/riders/ride-requests/tutorials/widget/migration-to-deep-links)
- MCP: a directory lists a remote endpoint `https://mcp.uber.com/claude/rides-3p/mcp` with OAuth, but the tool list was not confirmed (directory page returned 403 on fetch). Hearsay until checked against Uber docs. — [mcpservers.org via search](https://mcpservers.org/remote-mcp-servers/uber)
- Community MCP servers exist: one uses the user's own Uber developer-app credentials (OAuth), and `@striderlabs/mcp-uber` scrapes via browser automation (fare estimates reportedly available without login). The latter is unofficial automation of Uber's site. — [Strider Labs listing](https://mcpservers.org/th/servers/markswendsen-code/mcp-uber), [199-biotechnologies](https://mcpservers.org/fr/servers/199-biotechnologies/mcp-uber)

### Inferences
- Even if NEXUS obtained a token, showing Uber next to Ola/Rapido is exactly the comparison the terms prohibit. Showing Uber alone would be arguably allowed but needs approval.
- The only clean Uber path for a free desktop app is a deep link or m.uber.com link that opens the user's own Uber app or site with pickup and drop prefilled, so the user sees Uber's real price there.

### Gaps
- Whether the hosted Uber MCP endpoint is official, available in India, and returns estimates was not verified.
- Whether Uber India accepts new API partners in 2026 is unknown.
- Rate limits for the price endpoint were not found.

## Ola: API, Ola Maps, deep links

### Takeaway
No public fare-estimate API for individuals was found. Ola Maps (Krutrim) is a maps API (places, routing, tiles) with a free tier, not a fare API.

### Cited Findings
- Ola Maps offers 5 million free API calls per month to developers in India; the suite covers Navigation, Places, Tiles and Routing, hosted on Krutrim Cloud (2024 announcements). — [Neowin](https://www.neowin.net/news/indias-ola-maps-takes-on-google-maps-with-aggressive-pricing-and-roadmap/)
- A third-party directory lists an Ola ride-estimate/booking API as invite-only (contact affiliates@olacabs.com). This was not confirmed on Ola's own site. — [apis.io](https://apis.io/providers/ola/)

### Inferences
- Ola Maps routing could supply distance and duration for a local fare formula, but it will not return Ola's price.

### Gaps
- Ola deep-link format and current official developer terms were not retrieved.
- The company's current status in 2026 was not checked.

## Rapido

### Takeaway
No official public or partner fare API found. The only fare-estimate tooling is reverse-engineered, and it is reported to be breaking.

### Cited Findings
- A community "skill" documents an unofficial Rapido integration reverse-engineered from the Rapido PWA, using `POST /pricing/getFareEstimate` on `m.rapido.bike/pwa/api`. A later version says fare and booking responses now come back as encoded binary after a protobuf schema change. — [Skills Directory](https://www.skillsdirectory.com/skills/ankitjh4-rapido), [SkillsMP](https://skillsmp.com/skills/ankitjh4-indic-ai-skills-skills-rapido-skill-md)
- Rapido integrated with ONDC for Chennai metro tickets in Feb 2024 (not rides). — [Business Today / search summary](https://www.medianama.com/2025/05/223-uber-bets-india-ondc-stay-relevant/)
- No Rapido MCP server was found.

### Gaps
- Rapido deep-link format, any Rapido for Business API, and Rapido's terms were not checked.

## Namma Yatri / ONDC (Beckn)

### Takeaway
Beckn is an open protocol, and Namma Yatri is open source and ONDC-connected, so quotes come back via search/on_search. Hyderabad coverage and practical access for an individual developer were not established.

### Cited Findings
- Namma Yatri was the first mobility app onboarded to ONDC (Mar 2023), starting in Bengaluru; it is described as open source (AGPL-3.0) and built on Beckn. — [YourStory](https://yourstory.com/2023/03/ondc-onboards-auto-booking-app-namma-yatri), [Skills listing (thin source)](https://www.skillsdirectory.com/skills/ankitjh4-namma-yatri)
- In Beckn a buyer search is broadcast through a gateway to seller apps, which answer asynchronously with on_search messages. — [pdpspectra explainer (generic)](https://pdpspectra.com/blog/india-ondc-architecture-deep-dive)
- Uber joined ONDC for metro ticketing (Delhi) and signed an MoU to explore bus/metro; no evidence it publishes cab quotes on ONDC. — [Inc42](https://inc42.com/buzz/uber-partners-with-ondc-to-expand-its-mobility-offerings)

### Inferences
- Querying ONDC properly requires registering as a buyer app (BAP) with the network, which is a business onboarding, not a casual API key. This is my inference, not verified.

### Gaps
- Sandbox access, cost and whether Namma Yatri or other apps serve Hyderabad on ONDC.
- inDrive and BluSmart were not researched.

## Aggregators and compare apps

### Takeaway
Nothing found that offers a free-tier fare API covering Ola/Uber/Rapido in India.

### Cited Findings
- Search results surfaced only an unrelated Sydney airport taxi fare API on RapidAPI. — [RapidAPI](https://rapidapi.com/he/silvertaxisydneyairport/api/fare-estimate)

### Gaps
- Google Maps ride-hailing tab, Moovit, Rome2Rio, Cabbi: not investigated.

## Legality and ToS of scraping / UI automation

### Takeaway
Uber's own terms bar price comparison via the API. For scraping, reverse engineering and phone UI automation, I found no authoritative source; assume ToS violation and account risk.

### Cited Findings
- Uber API Terms §II B bars competitor price comparison. — [Uber docs](https://developer.uber.com/docs/riders/references/api/v1.2/estimates-price-get)
- Indian CCI/NCLAT/SC matters on Ola/Uber/Rapido concern price fixing and predatory pricing, not scraping. — [Inc42](https://inc42.com/buzz/cci-rejects-price-fixing-allegations-against-cab-aggregators-ola-and-uber)
- The Centre has probed Ola/Uber for different fares on Android versus iOS (consumer law). — [WION](https://www.wionews.com/india-news/uber-ola-face-govt-probe-over-allegations-of-differential-pricing-for-iphones-android-8647019/amp)

### Inferences
- Reverse-engineered endpoints (Rapido PWA, Uber web) break the apps' ToS and can change or be blocked without notice; flag as high risk. Not legal advice.

### Gaps
- No Indian case law or specific app ToS text on scraping was retrieved.

## Local fare formula: published Hyderabad / Telangana tariffs

### Takeaway
Autos have a government tariff and can be estimated fairly well. App cab and bike-taxi prices are dynamic, and no official Telangana per-km cab or bike-taxi tariff was found.

### Cited Findings
- Telangana revised auto fares in 2026: ₹30 for the first 1.6 km, ₹14 per extra km, night 11pm to 5am at 1.5x, waiting 75 paise per minute (previous: ₹20, ₹11/km). — [Deccan Chronicle](https://www.deccanchronicle.com/southern-states/telangana/state-revises-autorickshaw-fares-1992627), [NewsBytes](https://www.newsbytesapp.com/news/india/ponnam-prabhakar-announces-telangana-auto-fares-rs30-minimum-11pm-5am-15x/tldr)
- Conflict: another source gives Rs 30 for first 1.9 km then Rs 15/km for Hyderabad. — [Hyderabad Mail](https://hyderabadmail.com/hyderabad-auto-drivers-fare-hike-bike-taxi-ban-tgpwu/). The effective date and exact slab need checking with the Telangana transport department.
- RGIA prepaid cabs: reported Rs 21/km day, Rs 25/km night, minimum fare for up to 5 km Rs 143 (Rs 163 night). — [Telangana Today](https://telanganatoday.com/hyderabad-revision-of-fares-of-prepaid-cabs-operating-at-rgia)
- Central aggregator guidelines (2025) reportedly allow up to 2x base fare surge at peak. — [Telangana Today](https://telanganatoday.com/cab-aggregators-can-charge-up-to-2-times-the-base-fare-during-peak-hours)
- Unions claim a 2016 order set cabs at about ₹17/km; unverified. — [Deccan Chronicle](https://www.deccanchronicle.com/southern-states/telangana/hyderabad-auto-unions-press-for-fare-revision-1987102)
- Bike taxis in Hyderabad: unions demand a ban and call white-plate bike taxis illegal; I found no confirmed Telangana regulation or official bike-taxi tariff. — [Siasat](https://www.siasat.com/rapido-ola-uber-face-backlash-over-bike-taxis-in-hyderabad-3238894), [The Week](https://www.theweek.in/news/india/2025/06/25/telangana-riding-bike-taxi-illegal-gig-workers-union-raises-concerns.html)

### Inferences
- Reliability: autos good (regulated, though meter versus app rounding and extras vary), cabs and bikes poor (surge up to 2x, per-app pricing). A formula should show a range and say "estimate, not the app's price".
- Bike-taxi legality in Telangana is contested, so NEXUS should avoid implying it is officially sanctioned.

### Gaps
- Official Telangana notification for the auto revision (date, slab).
- Any app cab per-km or per-minute base rates for Hyderabad, and Uber/Ola/Rapido own rate cards.

## Recommendation

### Takeaway
Build a clearly labelled local estimate plus "open in app" deep links now; defer real per-app prices until a legitimate partner channel exists.

### Inferences
- Now: use Ola Maps (or the existing Google routing) for distance and time; compute an auto fare from the Telangana tariff and a wide-range estimate for cab and bike; label "Estimate, not a live price". Add a button that opens Uber via a setPickup deep link or m.uber.com with pickup and drop prefilled (with formatted address), and similar for Ola/Rapido once their link formats are verified.
- Wait: live Uber prices (needs approved access, and comparison is prohibited), Ola API (invite-only), Rapido (none), ONDC/Namma Yatri (needs BAP onboarding; check Hyderabad coverage).
- Avoid: scraping, reverse-engineered endpoints and phone UI automation (ToS violation and account-ban risk; not recommended).
