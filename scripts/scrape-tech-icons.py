#!/usr/bin/env python3
"""Download a logo for every technology that has no bundled brand mark.

Usage: python scripts/scrape-tech-icons.py [--only id,id] [--refresh]

For each technology in src-tauri/src/site_fingerprints.json (plus the app-scan ones named below) that
`frontend/src/sidebar/techIcons.json` does not cover, fetch the tool's own website and take, in order:
  1. <link rel="icon" type="image/svg+xml">   (a real vector mark)
  2. <link rel="apple-touch-icon"> / the largest <link rel="icon"> raster
  3. /favicon.svg, /favicon.png, /apple-touch-icon.png, /favicon.ico
Files go to frontend/public/tech-icons/scraped/<id>.<svg|png|ico> and frontend/src/sidebar/techIconsScraped.json
records { ext, source, tone }. These are identification marks fetched from the owner's own site, shown small next to
the tool's name; product marks belong to their owners and presence implies no affiliation. Pure HTTPS to the listed
sites, a browser-like User-Agent, 8 s timeouts, 300 KB cap, content sniffed (never trusted from the header alone).
Existing scraped files are kept unless --refresh. Safe to re-run; vendor-tech-icons.py never deletes scraped/.
"""
import html
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "frontend", "public", "tech-icons", "scraped")
MAP = os.path.join(ROOT, "frontend", "src", "sidebar", "techIconsScraped.json")
BUNDLED = os.path.join(ROOT, "frontend", "src", "sidebar", "techIcons.json")
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36"
MAX_BYTES = 300_000

# Technologies the scanner names that carry no website in the fingerprint table (desktop-app scan, headers).
EXTRA_SITES = {
    "chromium": "https://www.chromium.org",
    "cef": "https://bitbucket.org/chromiumembedded/cef",
    "nwjs": "https://nwjs.io",
    "wxwidgets": "https://www.wxwidgets.org",
    "imgui": "https://github.com/ocornut/imgui",
    "avalonia": "https://avaloniaui.net",
    "wpf": "https://learn.microsoft.com/dotnet/desktop/wpf/",
    "winforms": "https://learn.microsoft.com/dotnet/desktop/winforms/",
    "winui": "https://learn.microsoft.com/windows/apps/winui/",
    "msix": "https://learn.microsoft.com/windows/msix/",
    "clickonce": "https://learn.microsoft.com/visualstudio/deployment/clickonce-security-and-deployment",
    "gecko": "https://firefox-source-docs.mozilla.org/",
    "squirrel": "https://github.com/Squirrel/Squirrel.Windows",
    "innosetup": "https://jrsoftware.org/isinfo.php",
    "csp": "https://developer.mozilla.org/docs/Web/HTTP/CSP",
}
EXTRA_SITES.update({
    "jquery-migrate": "https://jquery.com", "jquery-validation": "https://jqueryvalidation.org",
    "jquery-ui": "https://jqueryui.com", "jquery-terminal": "https://terminal.jcubic.pl",
    "jplayer": "https://jplayer.org", "tinymce": "https://www.tiny.cloud", "yui": "https://yuilibrary.com",
    "prototypejs": "http://prototypejs.org", "easyxdm": "https://easyxdm.net", "plupload": "https://www.plupload.com",
    "flowplayer": "https://flowplayer.com", "dwr": "https://directwebremoting.org",
    "bootstrap-select": "https://developer.snapappointments.com/bootstrap-select/", "ckeditor": "https://ckeditor.com",
    "ckeditor5": "https://ckeditor.com/ckeditor-5/", "extjs": "https://www.sencha.com/products/extjs/",
    "markdown-it": "https://markdown-it.github.io", "jszip": "https://stuk.github.io/jszip/", "alasql": "http://alasql.org",
    "jquery-datatables": "https://datatables.net", "froala": "https://froala.com", "pendo": "https://www.pendo.io",
    "select2": "https://select2.org", "blueimp-file-upload": "https://blueimp.github.io/jQuery-File-Upload/",
    "ua-parser-js": "https://uaparser.dev", "mathjax": "https://www.mathjax.org", "pdf-js": "https://mozilla.github.io/pdf.js/",
    "pdfobject": "https://pdfobject.com", "highlightjs": "https://highlightjs.org", "marked": "https://marked.js.org",
    "echarts": "https://echarts.apache.org", "zrender": "https://ecomfe.github.io/zrender-doc/public/",
    "videojs": "https://videojs.com", "quill": "https://quilljs.com", "protobufjs": "https://protobufjs.github.io/protobuf.js/",
    "hammerjs": "https://hammerjs.github.io", "mol-view": "https://mol.hyoo.ru", "numeraljs": "http://numeraljs.com",
})
# A page that only offers its host's generic icon (GitHub, Bitbucket, a policy generator, a news site...) is not the
# tool's own mark: reject those sources so a wrong logo is never shown.
REJECT_SOURCE = (
    "githubassets.com", "github.com", "bitbucket", "atl-paas.net", "websitepolicies.com", "bbc.co.uk", "bbc.com",
    "developers.google.com", "gstatic.com", "sourceforge.net", "wordpress.com", "wp-content/uploads/cropped-icon-programing",
)

# Light marks that vanish on the white tile: show them on the dark one.
DARK_TONE = {"jquery-datatables", "marked", "termly", "sugar"}

# Headers and browser hints have no brand: the UI draws a glyph for them, scraping a docs site would be misleading.
SKIP = {
    "hsts", "x-frame-options", "x-content-type-options", "referrer-policy", "permissions-policy", "coep", "corp",
    "trusted-types", "expect-ct", "feature-policy", "gzip", "http3", "early-hints", "preload", "prefetch",
    "preconnect", "dns-prefetch", "modulepreload", "service-worker", "csp", "msix", "clickonce", "gecko",
    "winforms", "winui", "wpf",
    # the only site is a code host / generic docs page: no honest mark to show
    "bootstrap-select",  # its only site is a scheduling company's, whose icon is not the library's
    "cef", "imgui", "processingjs", "glow", "headjs", "closure", "swiffy", "fabricjs", "sproutcore", "mochikit",
}


def fetch(url, limit=MAX_BYTES):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": "*/*"})
    with urllib.request.urlopen(req, timeout=8) as r:
        data = r.read(limit + 1)
        if len(data) > limit:
            raise ValueError("too large")
        return data, r.geturl(), r.headers.get("Content-Type", "")


def sniff(data):
    head = data[:600].lstrip().lower()
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        return "png"
    if data[:4] == b"\x00\x00\x01\x00":
        return "ico"
    if b"<svg" in head or (head.startswith(b"<?xml") and b"<svg" in data[:2000].lower()):
        return "svg"
    if data[:3] == b"\xff\xd8\xff":
        return "jpg"
    if data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        return "webp"
    return None


def link_icons(page, base):
    """(priority, url) for every icon <link> in the page, best first."""
    found = []
    for tag in re.findall(r"<link\b[^>]*>", page, flags=re.I):
        rel = re.search(r'rel\s*=\s*["\']([^"\']+)["\']', tag, flags=re.I)
        href = re.search(r'href\s*=\s*["\']([^"\']+)["\']', tag, flags=re.I)
        if not rel or not href or "icon" not in rel.group(1).lower():
            continue
        if "mask-icon" in rel.group(1).lower():
            continue
        url = urllib.parse.urljoin(base, html.unescape(href.group(1)))
        typ = re.search(r'type\s*=\s*["\']([^"\']+)["\']', tag, flags=re.I)
        size = re.search(r'sizes\s*=\s*["\'](\d+)x\d+', tag, flags=re.I)
        px = int(size.group(1)) if size else 0
        if url.lower().split("?")[0].endswith(".svg") or (typ and "svg" in typ.group(1).lower()):
            pri = 0
        elif "apple-touch-icon" in rel.group(1).lower():
            pri = 1
        else:
            pri = 2 + (0 if px >= 64 else 1)
        found.append((pri, -px, url))
    return [u for _, _, u in sorted(found)]


def best_icon(site):
    host = urllib.parse.urlparse(site)
    base = f"{host.scheme}://{host.netloc}/"
    cands = []
    try:
        data, final, _ = fetch(site, 600_000)
        cands += link_icons(data.decode("utf-8", "ignore"), final)
        base = final
    except Exception as e:  # noqa: BLE001 - any failure just falls to the default paths
        print(f"    page: {type(e).__name__} {e}")
    for path in ("favicon.svg", "apple-touch-icon.png", "favicon.png", "favicon.ico"):
        cands.append(urllib.parse.urljoin(base, path))
    seen = set()
    for url in cands:
        if url in seen or not url.startswith("https://") or any(b in url for b in REJECT_SOURCE):
            continue
        seen.add(url)
        try:
            data, _, _ = fetch(url)
        except Exception:  # noqa: BLE001
            continue
        kind = sniff(data)
        if kind and len(data) > 120:
            return data, kind, url
    return None


def main():
    only = None
    refresh = "--refresh" in sys.argv
    if "--only" in sys.argv:
        only = set(sys.argv[sys.argv.index("--only") + 1].split(","))
    bundled = json.load(open(BUNDLED, encoding="utf-8"))
    techs = json.load(open(os.path.join(ROOT, "src-tauri", "src", "site_fingerprints.json"), encoding="utf-8"))["technologies"]
    sites = {t["id"]: t.get("website") for t in techs}
    for k, v in EXTRA_SITES.items():
        if not sites.get(k):
            sites[k] = v
    os.makedirs(OUT, exist_ok=True)
    done = json.load(open(MAP, encoding="utf-8")) if os.path.exists(MAP) else {}
    todo = [i for i in sites if i not in bundled and i not in SKIP and (only is None or i in only)]
    print(f"{len(todo)} technologies without a bundled mark")
    got = fail = 0
    for tid in todo:
        site = sites[tid]
        if not site:
            print(f"- {tid}: no website")
            fail += 1
            continue
        if tid in done and not refresh and os.path.exists(os.path.join(OUT, f"{tid}.{done[tid]['ext']}")):
            got += 1
            continue
        print(f"- {tid}: {site}")
        res = best_icon(site)
        if not res:
            print("    no icon found")
            fail += 1
            continue
        data, kind, url = res
        open(os.path.join(OUT, f"{tid}.{kind}"), "wb").write(data)
        done[tid] = {"ext": kind, "source": url, "tone": "dark" if tid in DARK_TONE else "light"}
        print(f"    {kind} {len(data)} B  <- {url}")
        got += 1
    json.dump(dict(sorted(done.items())), open(MAP, "w", encoding="utf-8"), indent=1)
    print(f"done: {got} with an icon, {fail} without (they get a glyph)")


if __name__ == "__main__":
    main()
