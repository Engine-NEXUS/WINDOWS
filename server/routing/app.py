"""NEXUS routing adapter: a tiny HTTP service in front of Valhalla.

NEXUS (the place card) asks this server for the road distance and the car and motorbike time between two points.
The adapter calls Valhalla twice (costing "auto" and "motor_scooter") and answers with one small JSON document.
Free to run, no per-request cost, no usage limit. Times are normal-traffic times, not live traffic.

    GET /route?fromLat=..&fromLon=..&toLat=..&toLon=..   Authorization: Bearer <ROUTING_TOKEN>
    -> {"km": 22.6, "carMin": 48, "bikeMin": 41, "bikeMeasured": true, "traffic": false}
    GET /health -> {"ok": true}

Environment: ROUTING_TOKEN (required), VALHALLA_URL (default http://valhalla:8002), HOST, PORT.
Standard library only.
"""
import hmac
import json
import os
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

MAX_KM = 4000.0


def parse_point(q, lat_key, lon_key):
    """('fromLat','fromLon') query values -> (lat, lon), or None when missing or not a real position."""
    try:
        lat, lon = float(q[lat_key][0]), float(q[lon_key][0])
    except (KeyError, IndexError, ValueError):
        return None
    if not (-90.0 <= lat <= 90.0 and -180.0 <= lon <= 180.0) or (lat == 0.0 and lon == 0.0):
        return None
    return lat, lon


def valhalla_summary(post, costing, a, b):
    """One Valhalla route -> (km, minutes), or None when Valhalla has no route for that costing."""
    body = {
        "locations": [{"lat": a[0], "lon": a[1]}, {"lat": b[0], "lon": b[1]}],
        "costing": costing,
        "directions_options": {"units": "kilometers"},
    }
    try:
        resp = post("/route", body)
        s = resp["trip"]["summary"]
        km, secs = float(s["length"]), float(s["time"])
    except (KeyError, TypeError, ValueError, urllib.error.URLError, OSError):
        return None
    if not (0 < km < MAX_KM) or secs <= 0:
        return None
    return km, max(1, round(secs / 60.0))


def route_answer(post, a, b):
    """The adapter's answer for two points; None when even the car route does not exist."""
    car = valhalla_summary(post, "auto", a, b)
    if car is None:
        return None
    bike = valhalla_summary(post, "motor_scooter", a, b)
    out = {"km": round(car[0], 1), "carMin": car[1], "traffic": False}
    if bike is not None:
        out["bikeMin"] = bike[1]
        out["bikeMeasured"] = True
    return out


def token_ok(header, token):
    """Constant-time check of 'Authorization: Bearer <token>'. An empty configured token never matches."""
    if not token or not header or not header.startswith("Bearer "):
        return False
    return hmac.compare_digest(header[7:].encode(), token.encode())


def make_valhalla_post(base):
    def post(path, body):
        req = urllib.request.Request(base.rstrip("/") + path, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as r:
            return json.loads(r.read().decode())

    return post


def make_handler(token, post):
    class Handler(BaseHTTPRequestHandler):
        server_version = "nexus-routing/1"

        def log_message(self, fmt, *args):  # no coordinates in logs
            return

        def _send(self, code, obj):
            data = json.dumps(obj).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            u = urlparse(self.path)
            if u.path == "/health":
                return self._send(200, {"ok": True})
            if u.path != "/route":
                return self._send(404, {"error": "not found"})
            if not token_ok(self.headers.get("Authorization"), token):
                return self._send(401, {"error": "bad token"})
            q = parse_qs(u.query)
            a, b = parse_point(q, "fromLat", "fromLon"), parse_point(q, "toLat", "toLon")
            if a is None or b is None:
                return self._send(400, {"error": "bad coordinates"})
            ans = route_answer(post, a, b)
            if ans is None:
                return self._send(404, {"error": "no route"})
            return self._send(200, ans)

    return Handler


def main():
    token = os.environ.get("ROUTING_TOKEN", "")
    if not token:
        raise SystemExit("Set ROUTING_TOKEN (a long random string) before starting.")
    post = make_valhalla_post(os.environ.get("VALHALLA_URL", "http://valhalla:8002"))
    host, port = os.environ.get("HOST", "0.0.0.0"), int(os.environ.get("PORT", "8080"))
    ThreadingHTTPServer((host, port), make_handler(token, post)).serve_forever()


if __name__ == "__main__":
    main()
