#!/usr/bin/env python3
"""Import permissively licensed fingerprints into src-tauri/src/site_fingerprints.json.

Sources (both pinned to a commit, see THIRD_PARTY_NOTICES.md):
  * Retire.js jsrepository.json (Apache-2.0): script file names, script URLs and script
    banners, each with a version capture.
  * Library Detector for Chrome libraries.js (MIT): page globals that only a given library
    sets, read by the hidden render pass ("js" signals), with a version path where one exists.

Wappalyzer-derived data (enthec/webappanalyzer, HTTPArchive/wappalyzer, ...) is GPL-3.0 and is
deliberately NOT imported: NEXUS ships no GPL code or data.

The import is repeatable: signals and technologies added by an earlier run are removed first
(imported signals end their evidence with " [Retire.js]" / " [Library Detector]"; imported
technologies carry "source"). Hand-written signals are never touched.

Usage:
  python scripts/import_fingerprints.py --src DIR   # DIR holds (or receives) the two source files
  python scripts/import_fingerprints.py --src DIR --dry-run
"""
import argparse
import json
import os
import re
import sys
import urllib.request

RETIRE_SHA = "35dca7b367884e571528203f1bf86928707671f5"
LD_SHA = "4838701f6cc04bb5a94d7a19bba1e6318c7707c0"
RETIRE_URL = f"https://raw.githubusercontent.com/RetireJS/retire.js/{RETIRE_SHA}/repository/jsrepository.json"
LD_URL = f"https://raw.githubusercontent.com/johnmichel/Library-Detector-for-Chrome/{LD_SHA}/library/libraries.js"

RETIRE_TAG = " [Retire.js]"
LD_TAG = " [Library Detector]"
TABLE = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "src", "site_fingerprints.json")

# Retire.js's own version placeholder value.
VERSION_RE = r"[0-9][0-9.a-z_\-]+"
PLACEHOLDER = "(§§version§§)"

# Retire.js entries that are not something a person would call part of a site's stack
# (internal React packages, the example entry).
RETIRE_SKIP = {"retire-example", "react-is", "scheduler"}
# Retire.js / Library Detector names whose table id differs.
ALIASES = {
    "@angular/core": "angular",
    "angularjs": "angularjs",
    "moment.js": "momentjs",
    "moment": "momentjs",
    "backbone.js": "backbone",
    "underscore.js": "underscore",
    "react-dom": "react",
    "three.js": "threejs",
    "three": "threejs",
    "gsap": "gsap",
    "greensock js": "gsap",
    "ember": "ember",
    "ember.js": "ember",
    "vue": "vue",
    "vue.js": "vue",
    "alpine.js": "alpine",
    "lit": "lit",
    "svelte": "svelte",
    "lodash": "lodash",
    "lo-dash": "lodash",
    "jquery": "jquery",
    "next.js (fast path)": "nextjs",
    "next-fast": "nextjs",
    "prototype": "prototypejs",
    "mustache.js": "mustache",
    "yui3": "yui",
    "video.js": "videojs",
    "jquery-ui-dialog": "jquery-ui",
    "jquery-ui-autocomplete": "jquery-ui",
    "jquery-ui-tooltip": "jquery-ui",
    "dojox": "dojo",
    "litelement": "lit",
    "lit-html": "lit",
}
# Category for technologies the table does not have yet (default "js-library").
CATEGORY = {
    "zurb": "css-framework",
    "foundation": "css-framework",
    "bootstrap": "css-framework",
    "tinymce": "ui-library",
    "ckeditor": "ui-library",
    "jquery-ui": "ui-library",
    "jquery-mobile": "ui-library",
    "highcharts": "ui-library",
    "chartjs": "ui-library",
    "d3": "ui-library",
    "videojs": "video",
    "jplayer": "video",
    "flowplayer": "video",
    "mediaelement": "video",
    "pendo": "analytics",
    "ezoic": "advertising",
    "boomerangjs": "monitoring",
    "webfontloader": "fonts",
    "materialdesignlite": "css-framework",
    "kendoui": "ui-library",
}
# Single-segment globals that are too generic to name a library on their own.
GENERIC_GLOBALS = {"$", "_", "jQuery", "app", "App", "React", "Vue", "angular", "ng", "define", "require", "module", "exports", "Ink"}

JS_PATH = re.compile(r"^[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*){0,4}$")


def norm(s):
    return re.sub(r"[^a-z0-9]", "", s.lower())


def fetch(src, name, url):
    path = os.path.join(src, name)
    if not os.path.exists(path):
        print(f"downloading {url}")
        with urllib.request.urlopen(url, timeout=30) as r:
            data = r.read()
        with open(path, "wb") as f:
            f.write(data)
    with open(path, encoding="utf-8") as f:
        return f.read()


def literal_chars(pattern):
    """Alphanumeric characters a pattern requires literally (escapes and classes dropped)."""
    p = pattern.replace(PLACEHOLDER, "")
    p = re.sub(r"\\.", "", p)
    p = re.sub(r"\[[^\]]*\]", "", p)
    p = re.sub(r"\{\d+(,\d*)?\}", "", p)
    return len(re.findall(r"[A-Za-z0-9]", p))


def convert(pattern, filename=False):
    """Retire.js pattern -> Rust regex with the version as the only capture group, or None."""
    if PLACEHOLDER not in pattern or pattern.count(PLACEHOLDER) != 1:
        return None
    if re.search(r"\\[1-9]", pattern) or "(?=" in pattern or "(?!" in pattern or "(?<" in pattern:
        return None  # backreferences / lookaround: not supported by the Rust regex crate
    if literal_chars(pattern) < 4:
        return None  # too loose to name a library
    out, i, in_class = [], 0, False
    while i < len(pattern):
        c = pattern[i]
        if c == "\\":
            out.append(pattern[i : i + 2])
            i += 2
            continue
        if in_class:
            in_class = c != "]"
            out.append(c)
            i += 1
            continue
        if c == "[":
            in_class = True
            out.append(c)
            i += 1
            continue
        if pattern.startswith(PLACEHOLDER, i):
            out.append("(" + VERSION_RE + ")")
            i += len(PLACEHOLDER)
            continue
        if c == "(" and not pattern.startswith("(?", i):
            out.append("(?:")
            i += 1
            continue
        out.append(c)
        i += 1
    rx = "".join(out)
    if filename:
        # Retire.js matches a bare file name; NEXUS matches the whole script URL.
        rx = rx[1:] if rx.startswith("^") else rx
        rx = "(?:^|/)" + rx
        if rx.endswith("$") and not rx.endswith("\\$"):
            rx = rx[:-1] + "(?:$|[?#])"
    try:
        re.compile(rx)
    except re.error:
        return None
    return rx


def retire_signals(name, entry):
    ex = entry.get("extractors", {})
    sigs = []
    for pat in ex.get("filename", [])[:3]:
        rx = convert(pat, filename=True)
        if rx:
            sigs.append({"kind": "script_src", "regex": rx, "weight": 0.85, "evidence": f"{name} file name with version{RETIRE_TAG}", "version": True})
    for pat in ex.get("uri", [])[:3]:
        rx = convert(pat)
        if rx:
            sigs.append({"kind": "script_src", "regex": rx, "weight": 0.85, "evidence": f"{name} script URL with version{RETIRE_TAG}", "version": True})
    for pat in ex.get("filecontent", [])[:4]:
        rx = convert(pat)
        if rx:
            sigs.append({"kind": "script_text", "regex": rx, "weight": 0.9, "evidence": f"{name} banner in a script{RETIRE_TAG}", "version": True})
    for f in ex.get("func", []):
        path = f[len("window."):] if f.startswith("window.") else f
        if JS_PATH.match(path) and path.split(".")[0] not in {"require"}:
            sigs.append({"kind": "js", "name": path, "weight": 0.95, "evidence": f"{path} global (rendered){RETIRE_TAG}", "version": True})
    return sigs


LD_ENTRY = re.compile(r"^ {4}'([^']+)': \{", re.M)


def ld_entries(src):
    starts = [(m.start(), m.group(1)) for m in LD_ENTRY.finditer(src)]
    for n, (pos, name) in enumerate(starts):
        end = starts[n + 1][0] if n + 1 < len(starts) else len(src)
        body = src[pos:end]
        field = lambda k: (re.search(rf"\b{k}:\s*'([^']*)'", body) or [None, None])[1]
        yield name, field("id"), field("url"), body


def ld_signals(name, body):
    sigs = []
    cond = re.search(r"if\s*\(\s*([^()]*?)\s*\)\s*\{", body)
    presence = None
    if cond:
        terms = [t.strip() for t in cond.group(1).split("&&")]
        paths = []
        for t in terms:
            m = re.fullmatch(r"win\.([\w$.]+)", t)
            if not m:
                paths = []
                break
            paths.append(m.group(1))
        if paths:
            presence = max(paths, key=lambda p: (p.count("."), len(p)))
    version = None
    # Only a plain path ("version: win.Foo.VERSION || ..."), never an expression ("win.x.length > 1 ? ...").
    v = re.search(r"version:\s*win\.([\w$.]+)\s*(?:\|\||\}|,|;|$)", body, re.M)
    if v and JS_PATH.match(v.group(1)) and not v.group(1).endswith(".length"):
        version = v.group(1)
        # When the test is not a plain presence check, a version on a generic global ("_.VERSION") could
        # belong to another library (Lodash sets _.VERSION too): skip it.
        if presence is None and version.split(".")[0] in GENERIC_GLOBALS:
            version = None
    if presence and presence.endswith(".length"):
        presence = None
    if presence and JS_PATH.match(presence) and presence != version:
        single = "." not in presence
        if not (single and (presence in GENERIC_GLOBALS or len(presence) < 4)):
            w = 0.6 if single else 0.85
            sigs.append({"kind": "js", "name": presence, "weight": w, "evidence": f"{presence} global (rendered){LD_TAG}"})
    if version and not ("." not in version and version in GENERIC_GLOBALS):
        sigs.append({"kind": "js", "name": version, "weight": 0.9, "evidence": f"{version} global (rendered){LD_TAG}", "version": True})
    return sigs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    os.makedirs(args.src, exist_ok=True)
    retire = json.loads(fetch(args.src, "jsrepository.json", RETIRE_URL))
    ld = fetch(args.src, "libraries.js", LD_URL)

    with open(TABLE, encoding="utf-8") as f:
        table = json.load(f)
    techs = [t for t in table["technologies"] if "source" not in t]
    for t in techs:
        t["signals"] = [s for s in t["signals"] if not s["evidence"].endswith((RETIRE_TAG, LD_TAG))]
    by_key = {}
    for t in techs:
        by_key[norm(t["id"])] = t
        by_key[norm(t["name"])] = t

    def target(name, ident, website, source):
        alias = ALIASES.get(name.lower()) or ALIASES.get((ident or "").lower())
        for k in filter(None, [alias, ident, name]):
            t = by_key.get(norm(k))
            if t:
                return t, False
        new_id = alias or re.sub(r"[^a-z0-9-]+", "-", (ident or name).lower()).strip("-")
        t = {"id": new_id, "name": name, "category": CATEGORY.get(new_id, "js-library"), "signals": [], "implies": [], "source": source}
        if website:
            t["website"] = website
        techs.append(t)
        by_key[norm(new_id)] = t
        by_key[norm(name)] = t
        return t, True

    stats = {"merged": 0, "new": 0, "signals": 0, "skipped": 0}

    def add(t, sigs, created):
        have = {(s["kind"], s.get("name"), s.get("regex"), s.get("contains")) for s in t["signals"]}
        fresh = [s for s in sigs if (s["kind"], s.get("name"), s.get("regex"), s.get("contains")) not in have]
        if not fresh:
            if created:
                techs.remove(t)
            stats["skipped"] += 1
            return
        t["signals"].extend(fresh)
        stats["signals"] += len(fresh)
        stats["new" if created else "merged"] += 1

    for name, entry in retire.items():
        if name in RETIRE_SKIP:
            continue
        sigs = retire_signals(name, entry)
        if sigs:
            t, created = target(name, name, None, "retire.js")
            add(t, sigs, created)
    for name, ident, url, body in ld_entries(ld):
        sigs = ld_signals(name, body)
        if sigs:
            t, created = target(name, ident, url, "library-detector")
            add(t, sigs, created)

    # A technology must be detectable on its own (table lint): drop new ones with only weak signals.
    techs = [t for t in techs if "source" not in t or any(s["weight"] >= 0.30 for s in t["signals"])]
    names = [t["name"] for t in techs]
    dup = {n for n in names if names.count(n) > 1}
    if dup:
        sys.exit(f"duplicate names after import: {sorted(dup)}")
    table["technologies"] = techs
    print(json.dumps(stats), "technologies:", len(techs))
    if not args.dry_run:
        with open(TABLE, "w", encoding="utf-8", newline="\n") as f:
            json.dump(table, f, indent=2, ensure_ascii=False)
            f.write("\n")


if __name__ == "__main__":
    main()
