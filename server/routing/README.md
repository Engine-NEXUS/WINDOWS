# NEXUS routing server (optional, free, unlimited)

Gives the NEXUS place card the road distance and the car and **motorbike** time for India with no per-request cost,
no usage limit and no Google key. Times are normal-traffic times, not live traffic (Google's key is still the way to
get live traffic).

## What you run
- **Valhalla** builds routing tiles from the OpenStreetMap India extract (Geofabrik) and answers route requests,
  including a `motor_scooter` profile.
- **adapter** (`app.py`, standard-library Python) turns two Valhalla calls into one small answer for NEXUS and checks a
  secret token.

## Start
1. A machine that stays on, with Docker and a few GB of free disk.
2. `ROUTING_TOKEN=$(openssl rand -hex 32) docker compose up -d` in this folder. Keep the token.
3. The first start downloads and builds the India data (an hour or more). `curl http://localhost:8080/health` answers
   `{"ok": true}` straight away; routes work once Valhalla finishes building.
4. Reach it from NEXUS:
   - same home network: `http://<server's LAN address>:8080` (change the port mapping in the compose file first);
   - from anywhere: a Cloudflare Tunnel to `http://localhost:8080`, which gives an `https://` address and opens no router
     ports. Add Cloudflare Access if only some people may use it.
5. In NEXUS: Command Hub, Connections, **Distance and travel time**: enter the address, paste the token, Save token,
   **Test server**, then Save.

## Safety
- The token is required for every route. Plain `http://` is accepted by NEXUS only for this computer and private
  network addresses, so the token never crosses the internet unencrypted.
- Request logging is off in the adapter, so coordinates are not written anywhere.
- Rotate the token by changing `ROUTING_TOKEN`, restarting the adapter, and pasting the new one in NEXUS.

## Tests
`python -m unittest -q` in this folder (adapter only; no Docker needed).

## Not verified here
The compose file and the Valhalla image's variable names were written without running Docker, and the Valhalla answer
fields (`trip.summary.length` in km, `time` in seconds) come from its documentation. Run `Test server` in NEXUS after
the first build finishes; it exercises the whole chain.
