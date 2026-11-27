#!/usr/bin/env python3
"""Run Reverse Lab over a folder of REAL programs (extracted vendor downloads) and cross-check every PE against
pefile (import count + section count). Static only: files are read, never executed.
    python scripts/reverse_live/real_apps.py <folder-with-exe-dll-asar>      (needs `pip install pefile`)
Used for the 2026-11-27 real-world test: 7-Zip, Notepad++, PuTTY, Node.js, Electron/Chromium, Python embeddable,
SQLite tools, Sysinternals (Autoruns, ProcDump, PsTools), Newtonsoft.Json (.NET), UPX itself.
"""
import json, subprocess, time, sys
from pathlib import Path
import pefile
import os
ROOT=Path(__file__).resolve().parents[2]
BIN=str(ROOT/"src-tauri"/"reverse-core"/"target"/"release"/"nexus-reverse")
root=Path(sys.argv[1] if len(sys.argv)>1 else "realapps")
files=sorted(p for p in root.rglob("*") if p.is_file() and p.suffix.lower() in (".exe",".dll",".asar",".node") and p.stat().st_size>1000)
rows=[]
for p in files:
    t=time.time()
    r=subprocess.run([BIN,"analyse",str(p)],capture_output=True,text=True)
    dt=time.time()-t
    if r.returncode!=0: rows.append((p.relative_to(root),"ERR",dt)); continue
    a=json.loads(r.stdout)
    # independent cross-check on PE: import count + section count vs pefile
    chk=""
    if a["kind"]=="pe":
        try:
            pe=pefile.PE(str(p),fast_load=False)
            pi=sum(len(e.imports) for e in getattr(pe,"DIRECTORY_ENTRY_IMPORT",[]))
            ri=len(a["pe"]["imports"])
            chk="imp %s/%s sec %s/%s"%(ri,pi,len(a["pe"]["sections"]),len(pe.sections))
            ok=(ri==pi and len(a["pe"]["sections"])==len(pe.sections))
            chk+=" OK" if ok else " MISMATCH"
        except Exception as e: chk="pefile-fail"
    caps=[(h["name"],h["severity"]) for h in a["capabilities"] if h["severity"] in("high","medium")]
    pk=a["packer"]["level"] if a.get("packer") else "-"
    v=(a["pe"]["version"] or {}) if a["pe"] else {}
    rows.append((str(p.relative_to(root)),a["kind"],round(dt*1000),p.stat().st_size//1024,pk,len(a["evidence"]["items"]) if isinstance(a["evidence"],dict) else "?",caps,chk,v.get("product_name") or v.get("file_description") or "",a.get("asar") and (a["asar"].get("package_name"),a["asar"].get("file_count"))))

for r in rows:
    if r[1]=="ERR": print("ERR",r); continue
    print(f"{r[0][:46]:46} {r[1]:4} {r[2]:5}ms {r[3]:7}KB pack={r[4]:9} {r[7]}")
    for c in r[6][:6]: print("      ",c)
