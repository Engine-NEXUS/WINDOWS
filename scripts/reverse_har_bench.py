#!/usr/bin/env python3
"""R5 ground truth for HAR -> OpenAPI: three local mock APIs whose true endpoint tables are known, a
recorder that drives them with seeded, realistic traffic and writes real HAR files, and a scorer that
compares what `nexus-reverse har` infers against the truth.

    python scripts/reverse_har_bench.py official        # held-out seeds 101-110, writes r5 (the scored number)
    python scripts/reverse_har_bench.py run [--bin PATH] [--out reverse_results.json] [--seed 7] [--sessions 6]

APIs
  shop   REST, bearer auth, numeric + UUID ids, page/limit + limit/offset pagination, 16 endpoints
  forge  REST, cookie session auth, WORD path parameters (org/repo/username), cursor pagination, 16 endpoints
  gql    GraphQL on one endpoint (9 operations) + 2 REST endpoints, X-API-Key auth

The traffic generator is independent of the inference code: each "session" picks its own parameter
values and visits each endpoint with probability 0.8, so some endpoints are seen once or twice - exactly
what a real capture looks like. Nothing is tuned to pass; numbers are whatever comes out.

Metrics (reverse_scorecard.py row R5)
  f1            endpoint+method set F1 (GraphQL operations count as "POST /graphql#Name")
  template_acc  fraction of captured REST requests whose inferred template equals the true one
  field_recall  fraction of true request/response field paths found on the matching inferred endpoint
Also reported (not scored): auth-scheme accuracy, pagination detection, and the planted-secret leak count
(hard gate: every credential the recorder generated must be absent from the tool's whole output).
"""

import argparse
import hashlib
import json
import random
import re
import subprocess
import sys
import threading
import uuid
from http.client import HTTPConnection
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BIN = ROOT / "src-tauri" / "reverse-core" / "target" / "release" / "nexus-reverse"


# ─── truth tables ────────────────────────────────────────────────────────────
# (method, template, auth_kind, pagination_params, request_body_sample, response_sample_fn_name)

def now_iso(r):
    return f"2026-{r.randint(1, 12):02d}-{r.randint(1, 28):02d}T{r.randint(0, 23):02d}:00:00Z"


def product(r, pid=None):
    return {"id": pid or r.randint(1, 5000), "name": r.choice(["Widget", "Gadget", "Gizmo"]) + str(r.randint(1, 99)),
            "price": round(r.uniform(1, 90), 2), "tags": ["a", "b"], "created_at": now_iso(r),
            "seller": {"id": r.randint(1, 99), "name": "Acme"}}


SHOP = [
    ("GET", "/api/v1/products", "bearer", ["page", "limit"], None, lambda r, p: {"items": [product(r)], "total": 3, "page": 1}),
    ("POST", "/api/v1/products", "bearer", [], {"name": "Widget", "price": 9.5, "tags": ["x"]}, lambda r, p: product(r)),
    ("GET", "/api/v1/products/{id}", "bearer", [], None, lambda r, p: product(r, int(p["id"]))),
    ("PUT", "/api/v1/products/{id}", "bearer", [], {"name": "Widget2", "price": 11.0}, lambda r, p: product(r, int(p["id"]))),
    ("DELETE", "/api/v1/products/{id}", "bearer", [], None, lambda r, p: {"deleted": True}),
    ("GET", "/api/v1/products/{id}/reviews", "bearer", ["page"], None, lambda r, p: {"reviews": [{"id": r.randint(1, 99), "stars": 4, "body": "ok", "author": {"id": 3, "name": "Sam"}}]}),
    ("POST", "/api/v1/products/{id}/reviews", "bearer", [], {"stars": 5, "body": "great"}, lambda r, p: {"id": r.randint(1, 99), "stars": 5}),
    ("GET", "/api/v1/users/me", "bearer", [], None, lambda r, p: {"id": 7, "email": "me@example.com", "name": "Me", "plan": "pro"}),
    ("GET", "/api/v1/users/{id}/orders", "bearer", ["limit", "offset"], None, lambda r, p: {"orders": [{"id": r.randint(1, 999), "total": 12.5, "status": "paid"}], "limit": 20, "offset": 0}),
    ("GET", "/api/v1/users/{id}/orders/{orderId}", "bearer", [], None, lambda r, p: {"id": int(p["orderId"]), "total": 12.5, "status": "paid", "lines": [{"sku": "A1", "qty": 2}]}),
    ("POST", "/api/v1/orders", "bearer", [], {"product_id": 3, "quantity": 2, "coupon": "SAVE10"}, lambda r, p: {"id": str(uuid.UUID(int=r.getrandbits(128))), "status": "pending", "total": 25.0}),
    ("GET", "/api/v1/orders/{uuid}", "bearer", [], None, lambda r, p: {"id": p["uuid"], "status": "shipped", "tracking": {"carrier": "UPS", "code": "1Z999"}}),
    ("POST", "/api/v1/auth/login", "none", [], {"email": "me@example.com", "password": "__PASSWORD__"}, lambda r, p: {"access_token": "__JWT__", "expires_in": 3600, "user": {"id": 7}}),
    ("GET", "/api/v1/categories", "bearer", [], None, lambda r, p: [{"id": 1, "name": "Tools"}, {"id": 2, "name": "Toys"}]),
    ("GET", "/api/v1/search", "bearer", ["page"], None, lambda r, p: {"results": [{"type": "product", "id": 4, "score": 0.9}], "q": "x"}),
    ("GET", "/api/v1/health", "none", [], None, lambda r, p: {"status": "ok", "uptime": 12345}),
]

FORGE = [
    ("GET", "/orgs/{org}", "cookie", [], None, lambda r, p: {"login": p["org"], "id": 11, "plan": "free", "members": 9}),
    ("GET", "/orgs/{org}/repos", "cookie", ["cursor"], None, lambda r, p: {"repos": [{"name": "api", "stars": 4, "private": False}], "next_cursor": "abc"}),
    ("GET", "/orgs/{org}/repos/{repo}", "cookie", [], None, lambda r, p: {"name": p["repo"], "owner": p["org"], "stars": 4, "default_branch": "main", "license": None}),
    ("GET", "/orgs/{org}/repos/{repo}/issues", "cookie", ["state", "cursor"], None, lambda r, p: {"issues": [{"number": 3, "title": "Bug", "state": "open", "labels": ["bug"]}], "next_cursor": None}),
    ("POST", "/orgs/{org}/repos/{repo}/issues", "cookie", [], {"title": "New bug", "body": "steps", "labels": ["bug"]}, lambda r, p: {"number": r.randint(1, 500), "title": "New bug", "state": "open"}),
    ("GET", "/orgs/{org}/repos/{repo}/issues/{number}", "cookie", [], None, lambda r, p: {"number": int(p["number"]), "title": "Bug", "state": "open", "assignee": {"login": "dev"}, "comments": 2}),
    ("PATCH", "/orgs/{org}/repos/{repo}/issues/{number}", "cookie", [], {"state": "closed"}, lambda r, p: {"number": int(p["number"]), "state": "closed"}),
    ("POST", "/orgs/{org}/repos/{repo}/issues/{number}/comments", "cookie", [], {"body": "thanks"}, lambda r, p: {"id": r.randint(1, 9999), "body": "thanks", "created_at": now_iso(r)}),
    ("GET", "/users/{username}", "cookie", [], None, lambda r, p: {"login": p["username"], "id": 21, "bio": None, "followers": 3}),
    ("GET", "/users/{username}/starred", "cookie", ["cursor"], None, lambda r, p: {"starred": [{"org": "acme", "repo": "api"}], "next_cursor": None}),
    ("PUT", "/user/starred/{org}/{repo}", "cookie", [], None, lambda r, p: {"starred": True}),
    ("DELETE", "/user/starred/{org}/{repo}", "cookie", [], None, lambda r, p: {"starred": False}),
    ("GET", "/user", "cookie", [], None, lambda r, p: {"login": "me", "id": 1, "email": "me@example.com", "two_factor": True}),
    ("POST", "/session", "none", [], {"username": "me", "password": "__PASSWORD__"}, lambda r, p: {"ok": True, "user": {"login": "me"}}),
    ("GET", "/search/repositories", "cookie", ["q"], None, lambda r, p: {"total": 2, "items": [{"name": "api", "stars": 1}]}),
    ("GET", "/rate_limit", "cookie", [], None, lambda r, p: {"limit": 5000, "remaining": 4990, "reset": 1700000000}),
]

GQL_OPS = [
    ("query", "GetViewer", "viewer { id login email }", {}, {"viewer": {"id": "1", "login": "me", "email": "me@example.com"}}),
    ("query", "ListProjects", "projects(first: 20) { nodes { id name } }", {}, {"projects": {"nodes": [{"id": "p1", "name": "A"}]}}),
    ("query", "GetProject", "project(id: $id) { id name owner { login } }", {"id": "ID!"}, {"project": {"id": "p1", "name": "A", "owner": {"login": "me"}}}),
    ("mutation", "CreateProject", "createProject(name: $name) { id name }", {"name": "String!"}, {"createProject": {"id": "p9", "name": "N"}}),
    ("mutation", "UpdateProject", "updateProject(id: $id, name: $name) { id name }", {"id": "ID!", "name": "String!"}, {"updateProject": {"id": "p1", "name": "N"}}),
    ("mutation", "DeleteProject", "deleteProject(id: $id) { ok }", {"id": "ID!"}, {"deleteProject": {"ok": True}}),
    ("query", "ListTasks", "tasks(projectId: $projectId) { id title done }", {"projectId": "ID!"}, {"tasks": [{"id": "t1", "title": "x", "done": False}]}),
    ("mutation", "CreateTask", "createTask(projectId: $projectId, title: $title) { id title }", {"projectId": "ID!", "title": "String!"}, {"createTask": {"id": "t2", "title": "T"}}),
    ("mutation", "CompleteTask", "completeTask(id: $id) { id done }", {"id": "ID!"}, {"completeTask": {"id": "t1", "done": True}}),
]
GQL_REST = [
    ("GET", "/healthz", "none", [], None, lambda r, p: {"ok": True}),
    ("GET", "/api/config", "apikey", [], None, lambda r, p: {"features": {"beta": True}, "version": "2.4.1"}),
    ("POST", "/auth/token", "none", [], {"client_id": "web", "client_secret": "__SECRET__"}, lambda r, p: {"api_key": "__APIKEY__", "ttl": 600}),
]


def flat(v, prefix=""):
    out = set()
    if isinstance(v, dict):
        for k, x in v.items():
            p = f"{prefix}.{k}" if prefix else k
            out.add(p)
            out |= flat(x, p)
    elif isinstance(v, list):
        for x in v[:25]:
            out |= flat(x, prefix + "[]")
    return out


def norm(path):
    return re.sub(r"\{[^}]*\}", "{}", path)


def tmpl_regex(t):
    return re.compile("^" + re.sub(r"\\\{[^}]*\\\}", "[^/]+", re.escape(t)) + "$")


# ─── mock server ─────────────────────────────────────────────────────────────

class Mock:
    """Serves a truth table: path templates matched exactly like a real router would."""

    def __init__(self, table, gql=None):
        self.table = table
        self.gql = gql or {}
        self.rng = random.Random(1)
        outer = self

        class H(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def _send(self, code, body):
                data = json.dumps(body).encode()
                self.send_response(code)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Content-Length", str(len(data)))
                if "login" in self.path or self.path == "/session":
                    self.send_header("Set-Cookie", "sid=SERVERSESSIONCOOKIE0123456789; HttpOnly")
                self.end_headers()
                self.wfile.write(data)

            def handle_any(self):
                path = self.path.split("?")[0]
                n = int(self.headers.get("Content-Length") or 0)
                raw = self.rfile.read(n) if n else b""
                if outer.gql and self.command == "POST" and path == "/graphql":
                    req = json.loads(raw or b"{}")
                    op = outer.gql.get(req.get("operationName"))
                    return self._send(200, {"data": op} if op else {"errors": [{"message": "unknown"}]})
                for (m, t, _a, _p, _b, fn) in outer.table:
                    if m == self.command:
                        mm = tmpl_regex(t).match(path)
                        if mm:
                            names = re.findall(r"\{([^}]*)\}", t)
                            vals = path.strip("/").split("/")
                            tparts = t.strip("/").split("/")
                            params = {nm: vals[i] for i, seg in enumerate(tparts) if seg.startswith("{") for nm in [seg[1:-1]]}
                            return self._send(200, fn(outer.rng, params))
                self._send(404, {"error": "not found"})

            do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = handle_any

        self.srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.port = self.srv.server_address[1]
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()

    def close(self):
        self.srv.shutdown()


# ─── recorder ────────────────────────────────────────────────────────────────

class Recorder:
    def __init__(self, port, secrets):
        self.port = port
        self.entries = []
        self.labels = []  # (method, true template) per recorded REST request
        self.secrets = secrets

    def call(self, method, path, label_tmpl, headers=None, body=None, query=""):
        conn = HTTPConnection("127.0.0.1", self.port, timeout=5)
        url = path + (("?" + query) if query else "")
        hdrs = {"Accept": "application/json", "User-Agent": "bench/1.0", **(headers or {})}
        data = None
        if body is not None:
            data = json.dumps(body).encode()
            hdrs["Content-Type"] = "application/json"
        conn.request(method, url, body=data, headers=hdrs)
        resp = conn.getresponse()
        text = resp.read().decode()
        self.entries.append({
            "startedDateTime": "2026-01-01T00:00:00Z", "time": 5,
            "request": {"method": method, "url": f"http://127.0.0.1:{self.port}{url}", "httpVersion": "HTTP/1.1",
                        "headers": [{"name": k, "value": v} for k, v in hdrs.items()],
                        "queryString": [], "cookies": [], "headersSize": -1, "bodySize": len(data or b""),
                        **({"postData": {"mimeType": "application/json", "text": data.decode()}} if data else {})},
            "response": {"status": resp.status, "statusText": "OK", "httpVersion": "HTTP/1.1",
                         "headers": [{"name": k, "value": v} for k, v in resp.getheaders()],
                         "cookies": [], "content": {"size": len(text), "mimeType": resp.getheader("Content-Type", ""), "text": text},
                         "redirectURL": "", "headersSize": -1, "bodySize": len(text)},
            "cache": {}, "timings": {"send": 0, "wait": 5, "receive": 0},
        })
        self.labels.append((method, label_tmpl))

    def write(self, path):
        Path(path).write_text(json.dumps({"log": {"version": "1.2", "creator": {"name": "bench", "version": "1"}, "entries": self.entries}}))


def secrets_for(rng, tag):
    tok = lambda n: "".join(rng.choice("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789") for _ in range(n))
    jwt = "eyJhbGciOiJIUzI1NiJ9." + tok(40) + "." + tok(43)
    return {"jwt": jwt, "password": f"Pw-{tok(12)}!", "secret": f"cs_{tok(30)}", "apikey": f"sk_live_{tok(28)}", "cookie": f"S{tok(30)}"}


def fill(v, sec):
    s = json.dumps(v)
    for k, tag in (("jwt", "__JWT__"), ("password", "__PASSWORD__"), ("secret", "__SECRET__"), ("apikey", "__APIKEY__")):
        s = s.replace(tag, sec[k])
    return json.loads(s)


def pools(rng, n):
    return {
        "id": [str(rng.randint(10, 9999)) for _ in range(n)],
        "orderId": [str(rng.randint(100, 99999)) for _ in range(n)],
        "uuid": [str(uuid.UUID(int=rng.getrandbits(128))) for _ in range(n)],
        "number": [str(rng.randint(1, 400)) for _ in range(n)],
        "org": rng.sample(["acme", "globex", "initech", "umbrella", "hooli", "stark", "wayne", "wonka", "oscorp", "tyrell", "cyberdyne", "soylent"], n),
        "repo": rng.sample(["api", "web", "docs", "infra", "mobile", "cli", "sdk", "tools", "core", "ui", "auth", "billing"], n),
        "username": rng.sample(["alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi", "ivan", "judy", "mallory", "niaj"], n),
    }


def drive_rest(rec, table, rng, sessions, sec, auth_headers):
    for s in range(sessions):
        pl = pools(rng, sessions)
        sess = {k: v[s] for k, v in pl.items()}
        for (m, t, auth, pag, body, fn) in table:
            if rng.random() > 0.8:
                continue
            path = t
            for nm in re.findall(r"\{([^}]*)\}", t):
                path = path.replace("{" + nm + "}", sess[nm])
            q = []
            for p in pag:
                q.append(f"{p}={ {'page': rng.randint(1, 5), 'limit': 20, 'offset': rng.choice([0, 20, 40]), 'cursor': 'c' + str(rng.randint(100, 999)), 'state': rng.choice(['open', 'closed']), 'q': 'term' + str(rng.randint(1, 9))}[p] }")
            h = auth_headers(auth) if auth != "none" else {}
            rec.call(m, path, t, headers=h, body=fill(body, sec) if body else None, query="&".join(q))


def run(args):
    rng = random.Random(args.seed)
    out_dir = ROOT / "reverse_corpus" / "har"
    out_dir.mkdir(parents=True, exist_ok=True)
    results = {"endpoints": 0, "tp": 0, "fp": 0, "fn": 0, "requests": 0, "template_ok": 0, "fields_total": 0, "fields_found": 0,
               "auth_ok": 0, "auth_n": 0, "pag_ok": 0, "pag_n": 0, "secret_leaks": 0, "secrets_planted": 0, "per_api": {}, "misses": []}
    for name in ("shop", "forge", "gql"):
        sec = secrets_for(rng, name)
        table = {"shop": SHOP, "forge": FORGE, "gql": GQL_REST}[name]
        gql = {op[1]: op[4] for op in GQL_OPS} if name == "gql" else None
        mock = Mock(table, gql)
        rec = Recorder(mock.port, sec)
        auth_headers = {
            "shop": lambda a: {"Authorization": f"Bearer {sec['jwt']}"},
            "forge": lambda a: {"Cookie": f"sid={sec['cookie']}; theme=dark"},
            "gql": lambda a: {"X-API-Key": sec["apikey"]},
        }[name]
        drive_rest(rec, table, rng, args.sessions, sec, auth_headers)
        truth = {(m, norm(t)): (t, a, p, b, fn) for (m, t, a, p, b, fn) in table}
        truth_ops = set()
        if name == "gql":
            for s in range(args.sessions):
                for (kind, opname, sel, vars_, resp) in GQL_OPS:
                    if rng.random() > 0.8:
                        continue
                    decl = ", ".join(f"${k}: {v}" for k, v in vars_.items())
                    q = f"{kind} {opname}{'(' + decl + ')' if decl else ''} {{ {sel} }}"
                    variables = {k: f"{k}-{s}" for k in vars_}
                    rec.call("POST", "/graphql", "/graphql", headers=auth_headers("apikey"), body={"operationName": opname, "query": q, "variables": variables})
                    truth_ops.add(opname)
            rec.labels = [(m, t) for (m, t) in rec.labels if t != "/graphql"] if False else rec.labels
        har_path = out_dir / f"{name}.har"
        rec.write(har_path)
        mock.close()
        planted = [v for v in sec.values()] + ["SERVERSESSIONCOOKIE0123456789"]
        proc = subprocess.run([str(args.bin), "har", str(har_path)], capture_output=True, text=True)
        if proc.returncode != 0:
            print(f"{name}: tool failed: {proc.stderr[:300]}")
            return 2
        got = json.loads(proc.stdout)
        leaked = [s for s in planted if s in proc.stdout]
        # ---- endpoint set ----
        pred = {}
        for e in got["endpoints"]:
            if e["graphql"]:
                continue
            pred[(e["method"], norm(e["path"]))] = e
        pred_ops = {o["name"] for o in got["graphql_ops"]}
        T = set(truth) | {("POST", f"/graphql#{o}") for o in truth_ops}
        P = set(pred) | {("POST", f"/graphql#{o}") for o in pred_ops}
        tp = len(T & P)
        results["tp"] += tp
        results["fp"] += len(P - T)
        results["fn"] += len(T - P)
        results["endpoints"] += len(T)
        for k in sorted(T - P):
            results["misses"].append(f"{name}: missing {k[0]} {k[1]}")
        for k in sorted(P - T):
            results["misses"].append(f"{name}: spurious {k[0]} {k[1]}")
        # ---- per-request template accuracy (REST only) ----
        regs = [(e, tmpl_regex(e["path"])) for e in got["endpoints"] if not e["graphql"]]
        ok = n = 0
        for entry, (m, true_t) in zip(rec.entries, rec.labels):
            if true_t == "/graphql":
                continue
            n += 1
            path = entry["request"]["url"].split("://", 1)[1].split("/", 1)[1].split("?")[0]
            path = "/" + path
            cands = [(e["path"].count("/") - e["path"].count("{"), e) for e, rx in regs if e["method"] == m and rx.match(path)]
            if cands:
                best = max(cands, key=lambda c: c[0])[1]
                ok += norm(best["path"]) == norm(true_t)
        results["requests"] += n
        results["template_ok"] += ok
        # ---- field recall ----
        rng2 = random.Random(5)
        for (m, t), (tt, a, p, body, fn) in truth.items():
            if (m, t) not in pred:
                # still count the fields as missing
                pass
            sample_params = {nm: "1" for nm in re.findall(r"\{([^}]*)\}", tt)}
            resp_f = flat(fn(rng2, sample_params))
            req_f = flat(fill(body, sec)) if body else set()
            e = pred.get((m, t))
            have_resp = set(e["response_fields"]) if e else set()
            have_req = set(e["request_fields"]) if e else set()
            results["fields_total"] += len(resp_f) + len(req_f)
            results["fields_found"] += len(resp_f & have_resp) + len(req_f & have_req)
            # auth + pagination (informational)
            if e is not None:
                results["auth_n"] += 1
                want = {"bearer": "http:bearer", "cookie": "apiKey:cookie:sid", "apikey": "apiKey:header:x-api-key"}.get(a)
                results["auth_ok"] += (want in e["auth"]) if want else (not e["auth"])
                results["pag_n"] += 1
                results["pag_ok"] += set(e["pagination"]) == {x for x in p if x in {"page", "limit", "offset", "cursor"}}
        if name == "gql":
            for kind, opname, sel, vars_, resp in GQL_OPS:
                if opname in truth_ops:
                    o = next((x for x in got["graphql_ops"] if x["name"] == opname), None)
                    # the operation's top-level field must be catalogued
                    want = re.match(r"\s*(\w+)", sel).group(1)
                    results["fields_total"] += 1
                    results["fields_found"] += bool(o and want in o["fields"])
        results["secret_leaks"] += len(leaked)
        results["secrets_planted"] += len(planted)
        results["per_api"][name] = {"truth": len(T), "predicted": len(P), "tp": tp, "requests": n, "template_ok": ok, "leaked": leaked,
                                    "redactions_reported": got["redactions"]}
    p = results["tp"] / max(1, results["tp"] + results["fp"])
    r = results["tp"] / max(1, results["tp"] + results["fn"])
    f1 = 2 * p * r / max(1e-9, p + r)
    r5 = {"endpoints": results["endpoints"], "f1": round(f1, 4), "precision": round(p, 4), "recall": round(r, 4),
          "template_acc": round(results["template_ok"] / max(1, results["requests"]), 4),
          "field_recall": round(results["fields_found"] / max(1, results["fields_total"]), 4),
          "requests": results["requests"], "auth_acc": round(results["auth_ok"] / max(1, results["auth_n"]), 3),
          "pagination_acc": round(results["pag_ok"] / max(1, results["pag_n"]), 3),
          "secret_leaks": results["secret_leaks"], "secrets_planted": results["secrets_planted"],
          "raw": {k: results[k] for k in ("tp", "fp", "fn", "requests", "template_ok", "fields_total", "fields_found", "auth_ok", "auth_n", "pag_ok", "pag_n", "secret_leaks", "secrets_planted", "endpoints")},
          "per_api": results["per_api"], "misses": results["misses"][:40], "seed": args.seed, "sessions": args.sessions}
    path = Path(args.out)
    cur = json.loads(path.read_text()) if path.exists() else {}
    cur["r5"] = r5
    path.write_text(json.dumps(cur, indent=2, default=list))
    print(json.dumps(r5, indent=2))
    return 0


HELD_OUT_SEEDS = [101, 102, 103, 104, 105, 106, 107, 108, 109, 110]  # never inspected while the inference was developed


def official(args):
    """Aggregate raw counts over the held-out seeds (the number the scorecard uses)."""
    import tempfile
    tot = {}
    per = []
    for sd in HELD_OUT_SEEDS:
        with tempfile.TemporaryDirectory() as td:
            a = argparse.Namespace(bin=args.bin, out=str(Path(td) / "r.json"), seed=sd, sessions=args.sessions)
            if run(a) != 0:
                return 2
            r = json.loads(Path(a.out).read_text())["r5"]
        per.append((sd, r["f1"], r["template_acc"], r["field_recall"]))
        for k, v in r["raw"].items():
            tot[k] = tot.get(k, 0) + v
    p = tot["tp"] / max(1, tot["tp"] + tot["fp"])
    rc = tot["tp"] / max(1, tot["tp"] + tot["fn"])
    f1 = 2 * p * rc / max(1e-9, p + rc)
    r5 = {"endpoints": tot["endpoints"] // len(HELD_OUT_SEEDS), "captures": len(HELD_OUT_SEEDS), "f1": round(f1, 4), "precision": round(p, 4), "recall": round(rc, 4),
          "template_acc": round(tot["template_ok"] / max(1, tot["requests"]), 4),
          "field_recall": round(tot["fields_found"] / max(1, tot["fields_total"]), 4),
          "requests": tot["requests"], "seeds": HELD_OUT_SEEDS, "sessions": args.sessions,
          "worst_seed_f1": min(x[1] for x in per), "per_seed": per,
          "auth_acc": round(tot["auth_ok"] / max(1, tot["auth_n"]), 3), "pagination_acc": round(tot["pag_ok"] / max(1, tot["pag_n"]), 3),
          "secret_leaks": tot["secret_leaks"], "secrets_planted": tot["secrets_planted"],
          "note": "3 mock APIs x 10 held-out seeds; sparse captures (<3 distinct values for a word parameter) cannot be generalised and stay literal"}
    path = Path(args.out)
    cur = json.loads(path.read_text()) if path.exists() else {}
    cur["r5"] = r5
    path.write_text(json.dumps(cur, indent=2, default=list))
    print(json.dumps(r5, indent=2))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=["run", "official"])
    ap.add_argument("--bin", default=str(DEFAULT_BIN))
    ap.add_argument("--out", default=str(ROOT / "reverse_results.json"))
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--sessions", type=int, default=6)
    args = ap.parse_args(argv)
    return official(args) if args.cmd == "official" else run(args)


if __name__ == "__main__":
    sys.exit(main())
