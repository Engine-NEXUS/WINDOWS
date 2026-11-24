# Photo sources for one representative photo of a named place (BYO-key / keyless desktop assistant, late 2026)

Research depth: about 13 tool calls. Several claims are unverified; those are marked "UNVERIFIED".

## Google Places API (New) Place Photos: mechanics, key, pricing, terms

### Takeaway
Google gives the best coverage for businesses and landmarks. The flow is Text Search or Place Details to get `photos[].name`, then a GET on `/media`. Each user needs their own key and a billing account. Photo names must not be cached, and `authorAttributions` must be shown when they are non-empty.

### Cited Findings
- The `photos[]` array comes from Place Details, Nearby Search or Text Search. The field mask must include `photos` or `places.photos`. Up to 10 photos are returned per place. The image URL is `https://places.googleapis.com/v1/NAME/media?key=KEY&maxWidthPx=...`, where NAME looks like `places/PLACE_ID/photos/REF`. — [Place Photos (New)](https://developers.google.com/maps/documentation/places/web-service/place-photos)
- `maxHeightPx` or `maxWidthPx` is required, 1 to 4800. By default the response is an HTTP redirect to the image. With `skipHttpRedirect=true` it returns JSON containing `photoUri`. — same page
- `authorAttributions[]` is always present but may be empty. If it has entries, the app "must include the additional attribution wherever you display the image". — same page
- "You cannot cache a photo name." Names can expire (403 or 400 errors), so always get them from a fresh response. Google recommends loading photos on demand and points to Section 3.2.3(b) of the Maps Platform Terms. — same page
- Pricing (global list, fetched from Google's page): Place Details Photos is Enterprise tier, free cap 1,000 events per month, then $7.00 per 1,000 up to 100k. Text Search Essentials (IDs Only) is unlimited and free. Text Search Pro has 5,000 free per month. Enterprise has 1,000 free per month. — [Google Maps pricing](https://developers.google.com/maps/billing-and-pricing/pricing)
- A third-party source says the old $200 monthly credit was replaced by per-SKU free caps on 1 March 2025. This is secondary. — [openplacesapi.com](https://openplacesapi.com/blog/google-places-api-pricing)
- The Maps Embed API is documented as free with unlimited use, but it still requires an API key and a billing account. — [Embed API overview](https://developers.google.com/maps/documentation/embed)
- Google's terms restrict caching, storing and scraping Maps content. Place IDs are the cacheable exception. I could not retrieve the full terms text. — [Maps service terms](https://cloud.google.com/maps-platform/terms/maps-service-terms) (fetch was truncated)

### Inferences
- A cheap pipeline is a Text Search with the `places.id,places.displayName,places.photos` field mask, then one `/media` call. The photos field may bump the Text Search SKU to Enterprise (UNVERIFIED), so the 1,000 per month free cap likely applies to the combined lookup. That is about 1,000 place cards per user per month on the free tier. A Place Details call with only `photos` would be billed as a Photos SKU.
- A user key must allow Places API (New). A key restricted to one API probably works for both search and `/media`, since both are Places API (New) endpoints. I could not confirm this (UNVERIFIED).
- Keep place IDs only and refetch photos on demand. Do not store photo bytes or names.
- A desktop app can show the image in a card without keeping files on disk. Keep it in memory for the session only, and show attribution.
- The keyless Maps embed (`google.com/maps?q=...&output=embed`) shows the map and may show a place panel with photos, but it cannot hand the app a photo URL. I found no documentation that it does.

### Gaps
- India-specific pricing (Google publishes a separate India price list; the search result only mentioned it exists). Not read.
- Whether the Photos SKU is billed once on the redirect (`/media`) call, and whether Text Search with the photos field upgrades the SKU. Not confirmed.
- Exact current caching wording in Section 3.2.3(b). Not retrieved.
- Whether key restriction to a single API works for photos. Not confirmed.
- Whether the keyless embed shows photos at all. Not confirmed.

## Wikipedia / Wikimedia Commons / Wikidata (keyless)

### Takeaway
The Wikipedia REST summary API is keyless and returns a thumbnail plus a Wikidata ID, which suits landmarks. Coverage for ordinary shops and restaurants is poor.

### Cited Findings
- `https://en.wikipedia.org/api/rest_v1/page/summary/Charminar` returned `thumbnail` (330px), `originalimage` (2174x3071), `wikibase_item` Q1771696 and `description` "Monument in India". — [REST summary API](https://en.wikipedia.org/api/rest_v1/page/summary/Charminar)
- The REST API is public and needs no key. RESTBase, the technology behind it, is "being deprecated", with the MediaWiki Core REST API as the successor. — [MediaWiki REST API](https://www.mediawiki.org/wiki/Wikimedia_REST_API)
- Wikimedia requires a descriptive User-Agent with contact information, e.g. `Client/version (contact) lib/version`. Generic or default agents may be blocked without notice. Browser JavaScript should use `Api-User-Agent`. — [User-Agent policy](https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_User-Agent_Policy)
- The `generator=geosearch` module with `prop=coordinates|pageimages` works through GeoData, which is installed on Commons. Generator parameters take a `g` prefix. — [API:Geosearch](https://www.mediawiki.org/wiki/API:Geosearch)
- Wikidata P625 is the coordinate property, and P18 is the image property. Wikidata tracks missing-image items by query. — [P625](https://dcmpx.remotevs.com/org/wikidata/www/SL/wiki/Property:P625)

### Inferences
- Reliable matching: first query Google or OSM for the name and coordinates, then run a Wikipedia geosearch within a small radius (about 200 to 500 m) and require a name similarity match. A name-only search risks wrong-place hits.
- Commons images carry per-file licences (CC BY, CC BY-SA, public domain), so the card should show author and licence from `extmetadata`. Use the pageimage thumbnail from the summary, with a link to the file page.
- Expect high hit rates for famous landmarks. Expect near zero for a local restaurant or shop.

### Gaps
- `wbsearchentities` parameters were not retrieved, and no count of P18 coverage for Indian businesses was found.
- The geosearch radius maximum was not confirmed. I recall about 10 km, but this is UNVERIFIED.
- The Commons-specific `imageinfo` thumbnail call was not tested.

## Mapillary, KartaView, OSM tags, Foursquare, Yelp and other APIs

### Takeaway
Mapillary needs a free token and gives street-level photos near coordinates, not the place itself. Foursquare photos are paid. Yelp has no free tier and has little India coverage. OSM `wikimedia_commons` or `image` tags are sparse.

### Cited Findings
- Mapillary: all graph and tile requests need a client token (free from the developer dashboard). `bbox` is minLon,minLat,maxLon,maxLat. The docs say the box must be smaller than 0.01 degrees square. Field `thumb_1024_url` is available, with other sizes. Search limit is 10,000 per minute per app. — [Mapillary API docs](https://www.mapillary.com/developer/api-documentation)
- Mapillary user content is CC BY-SA unless stated otherwise (terms effective 15 February 2024). The suggested attribution is the title, a link, the user, and a note that it is licensed under CC-BY-SA. A creator username field was added to the API. A forum thread argues the terms conflict with CC licensing, which is unresolved. — [Mapillary Terms](https://mapillary.com/legal), [help article](https://help.mapillary.com/hc/en-us/articles/115001770409-CC-BY-SA-license-for-open-data), [forum](https://forum.mapillary.com/t/map-data-license/6126)
- Foursquare: photos are on Premium endpoints. Foursquare offers a sandbox with $200 credits, but third-party sources disagree on the free allowance (10,000 Pro calls vs 500 calls) and photo prices ($6.50 or $18.75 per 1,000). — [search summary](https://pricingsaas.com/companies/foursquare), [openplacesapi](https://openplacesapi.com/blog/fsq-os-places-free-place-search). The official Foursquare doc URL returned 404.
- Yelp: Fusion free access ended in 2024. Current tiers are about $229, $299 and $643 per month with a 30-day, 5,000-call trial. Photos need Enhanced (3 photos) or Premium (12). The source is a mirror and a third party. — [apievangelist](https://providers.apievangelist.com/providers/yelp/), [App Developer Magazine](https://appdevelopermagazine.com/yelp-fusion-api-outrageous-new-pricing/)

### Inferences
- Mapillary is a "what the street looks like here" fallback. It needs a bbox of about 0.0005 degrees around the point, and the nearest image may not show the building. A BYO free token is viable. Respect CC BY-SA attribution.
- Foursquare and Yelp are not suitable for BYO free keys.
- OSM tags (`wikimedia_commons`, `image`, `wikidata`) can be read from Overpass or Nominatim extratags without a key, but they are rare for shops (my estimate, UNVERIFIED).

### Gaps
- KartaView, Bing/Azure Maps, Mapbox, HERE: not researched. I have no verified information on their photo offerings or terms.
- The OSM tag coverage rate is unmeasured.
- Mapillary India density is unmeasured.
- Foursquare official pricing and India coverage were not verified.

## Keyless imagery fallback (satellite or map thumbnail)

### Takeaway
Esri World Imagery tiles can be fetched without a key, but the terms are unclear, so treat it as a best-effort fallback with attribution.

### Cited Findings
- An OSM wiki page says the Esri hosted basemap is available under CC BY 4.0 without transaction limits, with attribution. It also lists conditions (a free developer account, no revenue generation, under one million tiles per month) that may relate to a different context. — [OSM wiki Esri](https://wiki.openstreetmap.org/wiki/Esri)
- Esri community posts say the older free-use conditions have changed. An Esri forum question about key-free World Imagery limits had no official answer. — [Esri community](https://community.esri.com/t5/arcgis-location-platform-developers-ques/inquiry-about-world-imagery-service-integration/m-p/1567734)

### Inferences
- A 3x3 tile mosaic at zoom 17 to 18 around the point gives a satellite thumbnail. Show "Imagery © Esri and data providers". OSM tiles are for map-style thumbnails and have their own tile usage policy, which should be checked before bulk use (not researched).

### Gaps
- Official current Esri terms for key-free use were not found. Whether a desktop app counts as commercial is unknown.

## Recommended fallback chain and expected hit rates

### Takeaway
Order: (1) user's Google Places key if present, (2) Wikipedia summary or Commons geosearch, (3) Mapillary with a free token, (4) satellite thumbnail. The hit rates below are my estimates, not measured.

### Inferences
- Landmark: Wikipedia summary is the first try (keyless). Estimated 85 to 95% for well-known monuments; Google gives about 95%+.
- Restaurant, shop or mall in an Indian city: Wikipedia likely under 10% (malls and hotels higher). Google with a key likely 85 to 95%, since Maps has photos for most listed businesses (UNVERIFIED). Mapillary gives a street view, not the place, and works where coverage exists. The satellite thumbnail works about 100% but is not a photo of the place.
- Without a Google key, a business card should honestly show a map or satellite image rather than imply a photo of the place.
- Always display attribution (Google `authorAttributions`, Commons author and licence, Mapillary username with CC BY-SA, Esri). Do not persist Google photos or photo names.

### Gaps
- No measured hit rates for any source in Hyderabad or other Indian cities.
