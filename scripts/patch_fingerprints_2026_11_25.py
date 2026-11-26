#!/usr/bin/env python3
"""One-off, repeatable patch of site_fingerprints.json from the 2026-11-25 real-site evaluation.

What it does (every change comes from a miss or a wrong answer found on a captured real site, see
docs/research/reverse-engineer-4-star-2026-11-25.md):
  * moves entries to the category that is true (jQuery is a library, not a framework; routers and framework features
    are not frameworks; lodash/moment/core-js are libraries, not programming languages);
  * adds the generator-meta / header / cookie signals that were missing for WooCommerce, Framer, MkDocs, PHP and Azure
    App Service, and the technologies that no entry covered (Eleventy, Docsify, TYPO3, Statamic, Craft CMS,
    WordPress VIP, Varnish, Super);
  * is idempotent: it only adds a signal or entry when it is not already there.

Run:  python scripts/patch_fingerprints_2026_11_25.py
"""
import json
import os

TABLE = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "src", "site_fingerprints.json")
BS = chr(92)  # a backslash, kept out of the literals below so they stay readable

RECATEGORISE = {
    "jquery": "js-library",
    "react-router": "routing", "vue-router": "routing", "svelte-routing": "routing",
    "tanstack-router": "routing", "remix-router": "routing",
    "next-image": "framework-feature", "next-router": "framework-feature", "astro-routes": "framework-feature",
    "lodash": "js-library", "momentjs": "js-library", "polyfill-io": "js-library", "core-js": "js-library",
    "regenerator-runtime": "js-library", "babel-polyfill": "js-library", "zod": "js-library", "yup": "js-library",
}


def sig(kind, weight, evidence, **kw):
    s = {"kind": kind, **kw, "weight": weight, "evidence": evidence}
    return s


ADD_SIGNALS = {
    "woocommerce": [
        sig("meta", 0.95, "WooCommerce generator meta", name="generator", regex="(?i)woocommerce" + BS + "s+([0-9][0-9a-z.-]*)", version=True),
    ],
    "framer": [
        sig("meta", 0.95, "Framer generator meta", name="generator", contains="framer"),
        sig("header", 0.9, "Server: Framer", name="server", contains="framer/"),
        sig("html", 0.85, "Made in Framer comment", contains="made in framer"),
    ],
    "mkdocs": [
        sig("script_text", 0.85, "MkDocs theme shortcut keys", contains='"help": 191'),
        sig("html", 0.8, "Material for MkDocs component attribute", contains="data-md-component"),
    ],
    "php": [
        sig("html", 0.5, "links to .php pages", regex="(?i)href=[" + '"' + "'][^" + '"' + "'#?]+" + BS + ".php(?:[?#" + '"' + "'])"),
    ],
    "azure-app-service": [
        sig("cookie", 0.85, "ARRAffinity cookie (Azure App Service)", name="ARRAffinity"),
    ],
}

NEW_TECH = [
    {
        "id": "eleventy", "name": "Eleventy", "category": "meta-framework", "website": "https://www.11ty.dev",
        "signals": [sig("meta", 0.95, "Eleventy generator meta", name="generator", regex="(?i)eleventy[^0-9]*v?([0-9][0-9.]*)", version=True)],
        "implies": [],
    },
    {
        "id": "docsify", "name": "Docsify", "category": "meta-framework", "website": "https://docsify.js.org",
        "signals": [
            sig("script_src", 0.9, "docsify script from a CDN", regex="(?i)docsify(?:@[0-9.]+)?/(?:lib|dist)/"),
            sig("script_text", 0.9, "window.$docsify configuration", contains="$docsify"),
        ],
        "implies": [],
    },
    {
        "id": "typo3", "name": "TYPO3", "category": "cms", "website": "https://typo3.org",
        "signals": [
            sig("meta", 0.95, "TYPO3 generator meta", name="generator", contains="typo3"),
            sig("html", 0.8, "typo3conf path in HTML", contains="typo3conf/"),
            sig("html", 0.8, "typo3temp path in HTML", contains="typo3temp/"),
        ],
        "implies": ["php"],
    },
    {
        "id": "statamic", "name": "Statamic", "category": "cms", "website": "https://statamic.com",
        "signals": [
            sig("header", 0.95, "X-Powered-By: Statamic", name="x-powered-by", contains="statamic"),
            sig("meta", 0.95, "Statamic generator meta", name="generator", contains="statamic"),
        ],
        "implies": ["laravel", "php"],
    },
    {
        "id": "craft-cms", "name": "Craft CMS", "category": "cms", "website": "https://craftcms.com",
        "signals": [
            sig("header", 0.95, "X-Powered-By: Craft CMS", name="x-powered-by", contains="craft cms"),
            sig("cookie", 0.85, "CraftSessionId cookie", name="CraftSessionId"),
        ],
        "implies": ["php"],
    },
    {
        "id": "wordpress-vip", "name": "WordPress VIP", "category": "hosting", "website": "https://wpvip.com",
        "signals": [sig("header", 0.95, "X-Powered-By: WordPress VIP", name="x-powered-by", contains="wordpress vip")],
        "implies": ["wordpress"],
    },
    {
        "id": "varnish", "name": "Varnish", "category": "cdn", "website": "https://varnish-cache.org",
        "signals": [
            sig("header", 0.9, "X-Varnish header", name="x-varnish"),
            sig("header", 0.8, "Via: varnish", name="via", contains="varnish"),
        ],
        "implies": [],
    },
    {
        "id": "super-so", "name": "Super", "category": "cms", "website": "https://super.so",
        "signals": [sig("meta", 0.9, "Super generator meta", name="generator", regex="(?i)^super$")],
        "implies": [],
    },
]


def main():
    with open(TABLE, encoding="utf8") as fh:
        table = json.load(fh)
    techs = {t["id"]: t for t in table["technologies"]}
    changed = 0
    for tid, cat in RECATEGORISE.items():
        t = techs.get(tid)
        if t and t["category"] != cat:
            t["category"] = cat
            changed += 1
    for tid, sigs in ADD_SIGNALS.items():
        t = techs.get(tid)
        if t is None:
            raise SystemExit(f"unknown technology {tid}")
        have = {json.dumps({k: v for k, v in s.items() if k not in ("weight", "evidence")}, sort_keys=True) for s in t["signals"]}
        for s in sigs:
            key = json.dumps({k: v for k, v in s.items() if k not in ("weight", "evidence")}, sort_keys=True)
            if key not in have:
                t["signals"].append(s)
                changed += 1
    names = {t["name"].lower() for t in table["technologies"]}
    for t in NEW_TECH:
        if t["id"] in techs or t["name"].lower() in names:
            continue
        table["technologies"].append(t)
        changed += 1
    with open(TABLE, "w", encoding="utf8", newline="\n") as fh:
        json.dump(table, fh, indent=2, ensure_ascii=False)
        fh.write("\n")
    print(f"{changed} change(s); {len(table['technologies'])} technologies")


if __name__ == "__main__":
    main()
