#!/usr/bin/env python3
"""Score `nexus-reverse har` on REAL websites/APIs captured with scripts/reverse_live/capture.js.

  cd scripts/reverse_live && mkdir -p har/<set> && cp config_<set>.json har/<set>/config.json && cp capture.js har/<set>/
  (cd har/<set> && NODE_PATH=$(npm root -g) node capture.js)        # needs `playwright` + network access
  python scripts/reverse_live/score_sites.py <dev|second|fresh>

Ground truth = the route templates of the calls the driver made (route tables from each API's own docs; Petstore
also has its official OpenAPI spec). `dev` = sites the inference was tuned on; `second` = tuned after a first
failing look; `fresh` = scored once with the code frozen (the only independent number).
"""
import json, re, subprocess, sys, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
BIN = str(ROOT / "src-tauri" / "reverse-core" / "target" / "release" / "nexus-reverse")
SET = sys.argv[1] if len(sys.argv) > 1 else "fresh"
D = Path(__file__).parent / "har" / SET
cfg = json.load(open(D / "config.json"))
SEC = ["sk_test_" + "4eC39HqLyjWDarjtT1zdp7dc", "sk_test_" + "9Zy8Xw7Vv6Uu5Tt4Ss3Rr2Qq", "sk_test_" + "Qw3Er4Ty5Ui6Op7As8Df9Gh0J",
       "S3cretPw!zz", "SESSIONSECRETABCDEF123456", "dTE6cHcx", "emilyspass", "cityslicka", "pistol", "83r5^_",
       ".".join(["eyJhbGciOiJIUzI1NiJ9", "eyJzdWIiOiJ0ZXN0LXVzZXIiLCJyb2xlIjoiYWRtaW4ifQ", "c2lnbmF0dXJlLXNpZ25hdHVyZS1zaWduYXR1cmU"])]

def norm(p):
    p = re.sub(r"\{[^}]*\}", "{}", p)
    return p.rstrip("/") or "/"

def path_of(u):
    rest = u.split("://", 1)[1]
    p = "/" + rest.split("/", 1)[1].split("?")[0] if "/" in rest else "/"
    return p.rstrip("/") or "/"

def tmpl(site, u):
    """Ground-truth route template of a driven call (written from each API's documentation)."""
    p = path_of(u)
    # ---- dev set ----
    if site == "petstore":
        p = re.sub(r"^/api/v3", "", p)
        p = re.sub(r"^/pet/\d+(/|$)", lambda m: "/pet/{}" + m.group(1), p)
        p = re.sub(r"^/store/order/\d+$", "/store/order/{}", p)
        if p.startswith("/user/") and p.split("/")[2] not in ("login", "logout", "createWithList"):
            p = "/user/{}"
        return "/api/v3" + p
    if site == "jsonplaceholder":
        return re.sub(r"/(\d+)(?=/|$)", "/{}", p)
    if site == "dummyjson":
        p = re.sub(r"/(\d+)(?=/|$)", "/{}", p)
        return re.sub(r"^/products/category/[^/]+$", "/products/category/{}", p)
    if site == "pokeapi":
        return re.sub(r"^/api/v2/(pokemon|pokemon-species|type|ability|berry)/[^/]+$", r"/api/v2/\1/{}", p)
    if site == "httpbin":
        for pre in ("status", "delay", "anything"):
            if p.startswith(f"/{pre}/"):
                return f"/{pre}/{{}}"
        return re.sub(r"^/basic-auth/[^/]+/[^/]+$", "/basic-auth/{}/{}", p)
    if site == "wikipedia":
        p = re.sub(r"^/api/rest_v1/page/(summary|html)/[^/]+$", r"/api/rest_v1/page/\1/{}", p)
        return re.sub(r"^/api/rest_v1/feed/featured/\d+/\d+/\d+$", "/api/rest_v1/feed/featured/{}/{}/{}", p)
    # ---- second-look set ----
    if site == "openlibrary":
        return re.sub(r"^/(works|authors|isbn|books|subjects)/[^/]+\.json$", r"/\1/{}.json", p)
    if site == "swapi":
        return re.sub(r"^/api/(people|planets|films|starships|species|vehicles)/\d+$", r"/api/\1/{}", p)
    if site == "dogceo":
        return re.sub(r"^/api/breed/[^/]+/", "/api/breed/{}/", p)
    if site == "zippopotam":
        return "/{}/{}"
    if site == "reqres":
        return re.sub(r"^/api/(users|unknown)/\d+$", r"/api/\1/{}", p)
    if site == "themealdb":
        return re.sub(r"^/api/json/v1/1/", "/api/json/v1/{}/", p)
    # ---- fresh set ----
    if site == "rickmorty":
        return re.sub(r"^/api/(character|location|episode)/\d+$", r"/api/\1/{}", p)
    if site == "tvmaze":
        return re.sub(r"^/(shows|people|episodes)/\d+", r"/\1/{}", p)
    if site == "fakestore":
        p = re.sub(r"^/(products|users|carts)/\d+$", r"/\1/{}", p)
        return re.sub(r"^/products/category/[^/]+$", "/products/category/{}", p)
    if site == "artic":
        return re.sub(r"^/api/v1/(artworks|artists)/\d+$", r"/api/v1/\1/{}", p)
    if site == "hnalgolia":
        return re.sub(r"^/api/v1/(items|users)/[^/]+$", r"/api/v1/\1/{}", p)
    return p


TP = FP = FN = OK = N = 0
for site, s in cfg["sites"].items():
    har = D / f"{site}.har"
    if not har.exists():
        continue
    t = time.time()
    r = subprocess.run([BIN, "har", str(har)], capture_output=True, text=True)
    dt = time.time() - t
    if r.returncode != 0:
        print(site, "ERR", r.stderr[:100]); continue
    g = json.loads(r.stdout)
    driven = [(m, u) for m, u, h, b in s["calls"]]
    hosts = {u.split("://")[1].split("/")[0] for m, u in driven}
    T = {(m, norm(tmpl(site, u))) for m, u in driven}
    P = {(e["method"], norm(e["path"])) for e in g["endpoints"] if e["host"].split("://")[1] in hosts}
    tp = len(T & P); fp = P - T; fn = T - P
    regs = [(e, re.compile("^" + re.sub(r"\\\{[^}]*\\\}", "[^/]+", re.escape(e["path"])) + "$")) for e in g["endpoints"]]
    ok = 0
    for m, u in driven:
        p = path_of(u)
        c = [(e["path"].count("/") - e["path"].count("{"), e) for e, rx in regs
             if e["method"] == m and e["host"].split("://")[1] == u.split("://")[1].split("/")[0] and rx.match(p)]
        if c and norm(max(c, key=lambda x: x[0])[1]["path"]) == norm(tmpl(site, u)):
            ok += 1
    Pp = tp / max(1, len(P)); R = tp / max(1, len(T)); F = 2 * Pp * R / max(1e-9, Pp + R)
    TP += tp; FP += len(fp); FN += len(fn); OK += ok; N += len(driven)
    leak = [x for x in SEC if x in r.stdout]
    print(f"{site:14} {dt*1000:5.0f}ms har={har.stat().st_size//1024:5}KB eps={len(P):2} truth={len(T):2} F1={F:.2f} (P={Pp:.2f} R={R:.2f}) tmplAcc={ok}/{len(driven)} leaks={len(leak)}")
    if fp: print("     spurious:", sorted(fp)[:5])
    if fn: print("     missing :", sorted(fn)[:5])
p = TP / max(1, TP + FP); r_ = TP / max(1, TP + FN)
print(f"\nTOTAL [{SET}]: endpoints P={p:.3f} R={r_:.3f} F1={2*p*r_/max(1e-9,p+r_):.3f}   per-request template accuracy {OK}/{N} = {OK/max(1,N):.3f}")
