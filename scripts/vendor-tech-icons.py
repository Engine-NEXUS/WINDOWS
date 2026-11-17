#!/usr/bin/env python3
"""Bundle a local logo for every technology in src-tauri/src/site_fingerprints.json.

Usage: python scripts/vendor-tech-icons.py <simple-icons dir> <@iconify-json/logos dir> [more @iconify-json/<set> dirs ...]
Extra sets are a last-resort fallback, in the order given: devicon (MIT), vscode-icons (MIT), cib (CC0-1.0).

Sources (both CC0-1.0, so no attribution is required, credited anyway in THIRD_PARTY_NOTICES.md):
  - simple-icons 13.21.0   (single-colour marks + brand hex)
  - @iconify-json/logos 1.2.17 (full-colour marks)
Writes frontend/public/tech-icons/<id>.svg and frontend/src/sidebar/techIcons.json
({ "<tech id>": { "tone": "light" | "dark" } }). The tile shows the mark on white for "light"
(dark or coloured marks) and on a dark tile for "dark" (near-white marks). Technologies with no
brand mark (HTTP headers, protocols, obscure libraries) get no file: the UI draws a category glyph.
Removes its own earlier output first. Never hand-edit the output.
"""
import json
import os
import re
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
si_dir, lg_dir = sys.argv[1], sys.argv[2]
extra_dirs = sys.argv[3:]
OUT = os.path.join(ROOT, "frontend", "public", "tech-icons")
MAP = os.path.join(ROOT, "frontend", "src", "sidebar", "techIcons.json")

def si_slug(title):
    """simple-icons' own file-name rule (Next.js -> nextdotjs)."""
    import unicodedata
    t = title.lower().replace("+", "plus").replace(".", "dot").replace("&", "and")
    t = unicodedata.normalize("NFD", t).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]", "", t)


def norm(s):
    return re.sub(r"[^a-z0-9]", "", s.lower())

si_data = json.load(open(os.path.join(si_dir, "_data", "simple-icons.json"), encoding="utf-8"))
si_data = si_data["icons"] if isinstance(si_data, dict) else si_data
si_idx = {}
for e in si_data:
    slug = e.get("slug") or si_slug(e["title"])
    if not os.path.exists(os.path.join(si_dir, "icons", slug + ".svg")):
        alt = norm(e["title"])
        if not os.path.exists(os.path.join(si_dir, "icons", alt + ".svg")):
            continue
        slug = alt
    for k in {norm(e["title"]), slug}:
        si_idx[k] = (slug, e["hex"])

lg = json.load(open(os.path.join(lg_dir, "icons.json"), encoding="utf-8"))
lg_icons = lg["icons"]
lg_idx = {norm(k): k for k in lg_icons}
for a, v in lg.get("aliases", {}).items():
    lg_idx.setdefault(norm(a), v["parent"])
LG_W, LG_H = lg.get("width", 256), lg.get("height", 256)

def load_extra(d):
    """name -> (icons dict, width, height) for one @iconify-json set; wordmarks and folders are left out."""
    data = json.load(open(os.path.join(d, "icons.json"), encoding="utf-8"))
    info = json.load(open(os.path.join(d, "info.json"), encoding="utf-8"))
    rank = {"original": 0, "plain": 1}
    best = {}
    for name in data["icons"]:
        base = name
        for pre in ("file_type_", "file-type-"):
            if base.startswith(pre):
                base = base[len(pre):]
        if base.startswith(("folder_type_", "default_", "folder-type-")) or "wordmark" in base:
            continue
        r = 2
        for suf, rk in (("-original", 0), ("-plain", 1)):
            if base.endswith(suf):
                base, r = base[: -len(suf)], rk
                break
        key = norm(base)
        if key and (key not in best or r < best[key][0]):
            best[key] = (r, name)
    return {
        "prefix": info.get("prefix", ""),
        "idx": {k: v[1] for k, v in best.items()},
        "icons": data["icons"],
        "w": data.get("width", 16),
        "h": data.get("height", 16),
        "licence": info.get("license", {}).get("spdx", "?"),
        "dir": d,
    }


extras = [load_extra(d) for d in extra_dirs]

techs = json.load(open(os.path.join(ROOT, "src-tauri", "src", "site_fingerprints.json"), encoding="utf-8"))["technologies"]

# Technologies the desktop-app scan (app_probe.rs) names, read from its source so a new one is picked up
# on the next run, plus the common languages and tools a sidebar answer can mention.
_seen = {t["id"] for t in techs}
_app = open(os.path.join(ROOT, "src-tauri", "src", "app_probe.rs"), encoding="utf-8").read()
for _id, _name in re.findall(r'"([a-z0-9][a-z0-9-]*)",\s*"([^"]+)",\s*"(?:app-framework|ui-toolkit|runtime|packaging|browser-engine|engine)"', _app):
    if _id not in _seen:
        techs.append({"id": _id, "name": _name, "category": "app"})
        _seen.add(_id)
for _id, _name in [
    ("python", "Python"), ("java", "Java"), ("rust", "Rust"), ("go", "Go"), ("cpp", "C++"), ("csharp", "C#"),
    ("php", "PHP"), ("ruby", "Ruby"), ("kotlin", "Kotlin"), ("swift", "Swift"), ("typescript", "TypeScript"),
    ("javascript", "JavaScript"), ("dart", "Dart"), ("lua", "Lua"), ("scala", "Scala"), ("elixir", "Elixir"),
    ("haskell", "Haskell"), ("perl", "Perl"), ("r", "R"), ("zig", "Zig"), ("c", "C"), ("sql", "MySQL"),
    ("docker", "Docker"), ("kubernetes", "Kubernetes"), ("git", "Git"), ("linux", "Linux"), ("windows", "Windows"),
    ("android", "Android"), ("apple", "Apple"), ("postgresql", "PostgreSQL"), ("sqlite", "SQLite"), ("redis", "Redis"),
    ("tauri", "Tauri"), ("electron", "Electron"), ("deno", "Deno"), ("bun", "Bun"), ("vite", "Vite"),
    ("webpack", "webpack"), ("npm", "npm"), ("pnpm", "pnpm"), ("yarn", "Yarn"), ("cloudflare", "Cloudflare"),
    ("openai", "OpenAI"), ("anthropic", "Anthropic"), ("whatsapp", "WhatsApp"), ("gmail", "Gmail"), ("slack", "Slack"),
    ("notion", "Notion"), ("figma", "Figma"), ("spotify", "Spotify"), ("discord", "Discord"), ("zoom", "Zoom"),
    ("steam", "Steam"), ("vscode", "Visual Studio Code"), ("brave", "Brave"), ("firefox", "Firefox"),
    ("chrome", "Google Chrome"), ("edge", "Microsoft Edge"), ("telegram", "Telegram"), ("github", "GitHub"),
    ("gitlab", "GitLab"), ("postman", "Postman"), ("obsidian", "Obsidian"), ("vlc", "VLC media player"),
]:
    if _id not in _seen:
        techs.append({"id": _id, "name": _name, "category": "language"})
        _seen.add(_id)

def luminance(hexv):
    r, g, b = (int(hexv[i:i + 2], 16) / 255 for i in (0, 2, 4))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b

def candidates(t):
    n = t["name"]
    c = [n, t["id"], re.sub(r"\s*[\d.]+$", "", n), re.sub(r"\(.*?\)", "", n)]
    if n.split():
        c.append(n.split()[0])
    if t.get("website"):
        m = re.search(r"//(?:www\.)?([^/]+)", t["website"])
        if m:
            c.append(m.group(1).split(".")[0])
    return [norm(x) for x in c if norm(x)]

# Remove only this script's own earlier files; `scraped/` belongs to scrape-tech-icons.py.
if os.path.isdir(OUT):
    for _f in os.listdir(OUT):
        _p = os.path.join(OUT, _f)
        if os.path.isfile(_p):
            os.remove(_p)
os.makedirs(OUT, exist_ok=True)
result, missing, used_extra = {}, [], {}
for t in techs:
    svg, tone = None, "light"
    cands = candidates(t)

    def si_hit(k):
        return si_idx.get(k) or next((si_idx[k + s2] for s2 in ("dotjs", "js", "dotcom", "io") if k + s2 in si_idx), None)

    # square symbol marks first (readable at 22-32 px); the full-colour set is a fallback (it has wide wordmarks)
    hit = next((h for h in (si_hit(k) for k in cands) if h), None)
    if hit:
        slug, hexv = hit
        raw = open(os.path.join(si_dir, "icons", slug + ".svg"), encoding="utf-8").read()
        if luminance(hexv) > 0.82:  # near-white brand colour: put it on a dark tile
            tone = "dark"
        svg = raw.replace("<svg ", f'<svg fill="#{hexv}" ', 1)
    else:
        k = next((k for k in cands if k in lg_idx), None)
        if k:
            body = lg_icons[lg_idx[k]]
            w, h = body.get("width", LG_W), body.get("height", LG_H)
            svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}">{body["body"]}</svg>'
    if not svg:
        for ex in extras:
            k = next((k for k in cands if k in ex["idx"]), None)
            if k:
                body = ex["icons"][ex["idx"][k]]
                w, h = body.get("width", ex["w"]), body.get("height", ex["h"])
                svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}">{body["body"]}</svg>'
                used_extra.setdefault(ex["prefix"], 0)
                used_extra[ex["prefix"]] += 1
                break
    if svg:
        open(os.path.join(OUT, t["id"] + ".svg"), "w", encoding="utf-8").write(svg)
        result[t["id"]] = {"tone": tone}
    else:
        missing.append(t["id"])

# NEXUS' own bundled app marks (Activity Manager) fill the rest: Slack, WhatsApp, Excel, Teams, ...
_act = os.path.join(ROOT, "frontend", "public", "activity-logos")
for f in sorted(os.listdir(_act)):
    stem, ext = os.path.splitext(f)
    if ext == ".svg" and stem != "genericapp" and stem not in result:
        shutil.copy(os.path.join(_act, f), os.path.join(OUT, f))
        result[stem] = {"tone": "light"}
for ex in extras:
    for cand in ("license.txt", "LICENSE", "LICENSE.md"):
        pth = os.path.join(ex["dir"], cand)
        if os.path.exists(pth):
            shutil.copy(pth, os.path.join(OUT, f"LICENSE-{ex['prefix']}.txt"))
            break
shutil.copy(os.path.join(si_dir, "LICENSE.md"), os.path.join(OUT, "LICENSE-simple-icons.md"))
open(os.path.join(OUT, "README.md"), "w", encoding="utf-8").write(
    "Generated by `scripts/vendor-tech-icons.py` from simple-icons 13.21.0 (CC0-1.0) and "
    "@iconify-json/logos 1.2.17 (CC0-1.0). Product marks belong to their owners; presence implies no affiliation. "
    f"{len(result)} marks; {len(missing)} technologies have none and use a category glyph.\n"
)
json.dump(dict(sorted(result.items())), open(MAP, "w", encoding="utf-8"), indent=1)
print(f"{len(result)} icons, {len(missing)} without a brand mark; from extra sets: {used_extra}")
print("still missing:", " ".join(missing))
