#!/usr/bin/env python3
"""Import technology fingerprints from the nuclei-templates project (MIT) into site_fingerprints.json.

Source: https://github.com/projectdiscovery/nuclei-templates, folder http/technologies, MIT licence, pinned to one
commit (see THIRD_PARTY_NOTICES.md). Only templates that ask for the site's root page and match on words or regular
expressions in the page body or in a named response header become signals; templates that need another URL, a
favicon hash, a DSL expression or a binary body are skipped (NEXUS makes one guarded request, not a crawl).

Rules (each is checked by scripts/import_nuclei_fingerprints.test.py):
  * a word shorter than 8 characters is never a signal (loose substrings caused false positives before);
  * a template whose words must ALL match ("and") gets weight 0.45 per signal, so it needs two hits to reach "likely";
    a template that needs ANY word gets 0.82 (detected);
  * a technology that already exists in the table (same id or name) is left alone; hand-written entries win;
  * every imported signal ends its evidence with " [nuclei-templates]" and every imported technology carries
    "source": "nuclei-templates", so a re-run removes its own earlier output first and never touches anything else;
  * DENY lists templates found to fire on unrelated pages in the real-site evaluation (docs/research/...).

Usage:
  git clone --depth 1 --filter=blob:none --sparse https://github.com/projectdiscovery/nuclei-templates.git DIR
  (cd DIR && git sparse-checkout set http/technologies && git checkout c136408aab5ac8a31860fece88c557bd57cae541)
  python scripts/import_nuclei_fingerprints.py --src DIR [--dry-run]
"""
import argparse
import glob
import json
import os
import re
import sys

import yaml

NUCLEI_SHA = "c136408aab5ac8a31860fece88c557bd57cae541"
TAG = " [nuclei-templates]"
SOURCE = "nuclei-templates"
TABLE = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "src", "site_fingerprints.json")
MIN_WORD = 8
OR_WEIGHT = 0.82
AND_WEIGHT = 0.45
MAX_SIGNALS = 8
# Template ids (file stems) removed after the real-site evaluation showed false positives. Each needs a reason.
DENY = {
    # "stem": "reason",
}
# Template names that describe a kind of page, not a product ("Symfony Default Page", "Coming Soon Page"): they fire on
# unrelated sites, so they are never imported (found in the real-site evaluation, see the module docstring).
GENERIC_NAME = re.compile(r"default|coming soon|welcome|test page|placeholder|landing|error page|index of|under construction|maintenance", re.I)
HEADER_NAME = re.compile(r"^[A-Za-z][A-Za-z0-9-]{2,}$")


def slug(name: str) -> str:
    s = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
    return s[:48] or "tech"


def clean_name(name: str) -> str:
    n = re.sub(r"\s*[-:]\s*(detect(ion)?|technology detection|version detect)\s*$", "", name, flags=re.I)
    n = re.sub(r"^\s*(detect|detection of)\s+", "", n, flags=re.I)
    n = re.sub(r"\s+(detect|detection)\s*$", "", n, flags=re.I)
    return re.sub(r"\s+", " ", n).strip()


def category(tags: str, name: str) -> str:
    t = {x.strip().lower() for x in (tags or "").split(",")}
    n = name.lower()
    if "cms" in t or " cms" in n:
        return "cms"
    if t & {"ecommerce", "shop"} or "shop" in n or "store" in n:
        return "ecommerce"
    if "waf" in t:
        return "security"
    if "cdn" in t:
        return "cdn"
    if t & {"server", "httpd", "webserver"} or "web server" in n or "http server" in n:
        return "web-server"
    if "cloud" in t:
        return "hosting"
    return "web-application"


def squash(text: str) -> str:
    return re.sub(r"[^a-z0-9]", "", text.lower())


def names_the_product(word: str, product: str) -> bool:
    """A signal is specific only when the matched text contains the product's own name (or its first word).

    The first import (every word of 8+ characters) gave 74 false detections on 55 real sites ('Documentation' ->
    Symfony Default Page, '/themes/' -> Winter CMS, a generic generator regex -> Metatag CMS). Requiring the product
    name inside the matched text removes that class: the page must say what it is.
    """
    w = squash(word)
    full = squash(product)
    first = squash(product.split()[0]) if product.split() else ""
    return (len(full) >= 5 and full in w) or (len(first) >= 5 and first in w)


def usable_word(word) -> bool:
    return isinstance(word, str) and len(word.strip()) >= MIN_WORD and "{{" not in word and "\n" not in word.strip()


def convert(tpl: dict, stem: str):
    """Return a technology dict, or None when the template cannot be used."""
    http = tpl.get("http")
    if not isinstance(http, list) or len(http) != 1:
        return None
    req = http[0]
    paths = req.get("path") or []
    if not paths or not all(isinstance(p, str) and p.rstrip("/") == "{{BaseURL}}" for p in paths):
        return None
    matchers = req.get("matchers") or []
    if any(m.get("type") in ("dsl", "binary", "size", "xpath") for m in matchers):
        return None
    joined_and = (req.get("matchers-condition") or "or").lower() == "and"
    content = [m for m in matchers if m.get("type") in ("word", "regex")]
    if not content:
        return None
    # "and" across matchers, or "and" inside a word list, means several signals must agree.
    needs_many = (joined_and and len(content) > 1) or any((m.get("condition") or "or").lower() == "and" and len(m.get("words") or m.get("regex") or []) > 1 for m in content)
    weight = AND_WEIGHT if needs_many else OR_WEIGHT
    signals = []
    seen = set()
    for m in content:
        part = (m.get("part") or "body").lower()
        if part in ("body", "response", "all", "html"):
            kind = "html"
        elif part == "header":
            kind = "header"
        else:
            continue
        if m["type"] == "word":
            for w in m.get("words") or []:
                if not usable_word(w):
                    continue
                w = w.strip()
                if kind == "html":
                    key = ("html", w.lower())
                    if key in seen:
                        continue
                    seen.add(key)
                    signals.append({"kind": "html", "contains": w.lower(), "weight": weight, "evidence": f"page contains {w!r}{TAG}"})
                else:
                    if ":" in w:
                        name, value = (x.strip() for x in w.split(":", 1))
                        if HEADER_NAME.match(name) and len(value) >= 3:
                            key = ("header", name.lower(), value.lower())
                            if key not in seen:
                                seen.add(key)
                                signals.append({"kind": "header", "name": name.lower(), "contains": value.lower(), "weight": weight, "evidence": f"{name}: {value}{TAG}"})
                    elif HEADER_NAME.match(w) and ("-" in w or w.lower().startswith("x")):
                        key = ("header", w.lower(), "")
                        if key not in seen:
                            seen.add(key)
                            signals.append({"kind": "header", "name": w.lower(), "weight": weight, "evidence": f"{w} header present{TAG}"})
        else:  # regex
            for r in m.get("regex") or []:
                if not usable_word(r) or kind != "html":
                    continue
                try:
                    re.compile(r)
                except re.error:
                    continue
                # Rust's regex has no lookaround or backreferences: skip those patterns.
                if re.search(r"\(\?[=!<]|\\[1-9]", r):
                    continue
                key = ("rx", r)
                if key in seen:
                    continue
                seen.add(key)
                signals.append({"kind": "html", "regex": "(?i)" + r, "weight": weight, "evidence": f"page matches {r[:60]!r}{TAG}"})
    info0 = tpl.get("info") or {}
    product = clean_name(str(info0.get("name") or stem))
    signals = [sg for sg in signals if names_the_product(sg.get("contains") or sg.get("regex") or sg.get("name", ""), product) or (sg["kind"] == "header" and names_the_product(sg.get("name", "") + (sg.get("contains") or ""), product))]
    if not signals:
        return None
    # "and" semantics with a single signal would be unreachable; promote it.
    if needs_many and len(signals) == 1:
        signals[0]["weight"] = OR_WEIGHT
    info = tpl.get("info") or {}
    name = clean_name(str(info.get("name") or stem))
    if not name or GENERIC_NAME.search(name):
        return None
    return {
        "id": slug(name),
        "name": name,
        "category": category(str(info.get("tags") or ""), name),
        "website": "",
        "signals": signals[:MAX_SIGNALS],
        "implies": [],
        "source": SOURCE,
    }


def load_templates(src: str):
    out = []
    for f in sorted(glob.glob(os.path.join(src, "http", "technologies", "**", "*.yaml"), recursive=True)):
        stem = os.path.splitext(os.path.basename(f))[0]
        if stem in DENY:
            continue
        try:
            with open(f, encoding="utf8") as fh:
                tpl = yaml.safe_load(fh)
        except Exception:
            continue
        if isinstance(tpl, dict):
            out.append((stem, tpl))
    return out


def strip_previous(table: dict) -> int:
    before = len(table["technologies"])
    table["technologies"] = [t for t in table["technologies"] if t.get("source") != SOURCE]
    for t in table["technologies"]:
        t["signals"] = [s for s in t["signals"] if not str(s.get("evidence", "")).endswith(TAG)]
    return before - len(table["technologies"])


def run(src: str, table_path: str, dry: bool):
    with open(table_path, encoding="utf8") as fh:
        table = json.load(fh)
    removed = strip_previous(table)
    existing_ids = {t["id"] for t in table["technologies"]}
    existing_names = {t["name"].lower() for t in table["technologies"]}
    added = []
    skipped_existing = 0
    for stem, tpl in load_templates(src):
        tech = convert(tpl, stem)
        if tech is None:
            continue
        if tech["id"] in existing_ids or tech["name"].lower() in existing_names:
            skipped_existing += 1
            continue
        existing_ids.add(tech["id"])
        existing_names.add(tech["name"].lower())
        added.append(tech)
    table["technologies"].extend(added)
    print(f"removed {removed} earlier imports; added {len(added)} technologies; left {skipped_existing} already in the table alone")
    cats = {}
    for t in added:
        cats[t["category"]] = cats.get(t["category"], 0) + 1
    print("categories:", dict(sorted(cats.items(), key=lambda kv: -kv[1])))
    if not dry:
        with open(table_path, "w", encoding="utf8", newline="\n") as fh:
            json.dump(table, fh, indent=2, ensure_ascii=False)
            fh.write("\n")
    return added


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True)
    ap.add_argument("--table", default=TABLE)
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    if not os.path.isdir(os.path.join(a.src, "http", "technologies")):
        sys.exit("no http/technologies folder under --src")
    run(a.src, a.table, a.dry_run)
