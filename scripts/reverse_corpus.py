#!/usr/bin/env python3
"""Build the Reverse Lab measurement corpus (local only, git-ignored): REAL Windows PE files taken from
PyPI `win_amd64` wheels, so Reverse Lab is measured on binaries nobody tuned it for.

    python scripts/reverse_corpus.py fetch [--out reverse_corpus] [--max-mb 700]
    python scripts/reverse_corpus.py pack  [--out reverse_corpus] [--count 24]   (needs the `upx` tool)
    python scripts/reverse_corpus.py stats [--out reverse_corpus]

The files stay on this machine (they are third-party software; only their hashes/sources go in the
manifest). Needs network access to pypi.org / files.pythonhosted.org.
"""

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PE_EXTS = {".exe", ".dll", ".pyd", ".sys", ".ocx", ".cpl", ".scr"}

# Packages whose Windows wheels ship native binaries (many compilers/toolchains, native + .NET, small + large).
PACKAGES = [
    "numpy", "scipy", "pillow", "cryptography", "lxml", "pywin32", "pythonnet", "clr-loader", "cffi", "psutil",
    "pyzmq", "setuptools", "pyinstaller", "uv", "ruff", "ninja", "cmake", "onnxruntime", "grpcio", "markupsafe",
    "pyyaml", "regex", "orjson", "pydantic-core", "charset-normalizer", "aiohttp", "yarl", "multidict",
    "frozenlist", "greenlet", "matplotlib", "kiwisolver", "contourpy", "pandas", "bcrypt", "pynacl", "msgpack",
    "ujson", "zstandard", "rpds-py", "tokenizers", "opencv-python-headless", "pywinpty", "winpty", "comtypes",
]


def pip_download(pkg, dest):
    cmd = [sys.executable, "-m", "pip", "download", pkg, "--only-binary=:all:", "--no-deps", "--platform", "win_amd64",
           "--python-version", "3.12", "--implementation", "cp", "--abi", "cp312", "--abi", "abi3", "--abi", "none",
           "-d", str(dest), "--quiet", "--disable-pip-version-check"]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.returncode == 0


def extract_pes(wheel, out_dir, budget_bytes):
    got = []
    with zipfile.ZipFile(wheel) as z:
        for info in z.infolist():
            ext = Path(info.filename).suffix.lower()
            if ext not in PE_EXTS or info.file_size < 1024 or info.file_size > 120 * 1024 * 1024:
                continue
            if budget_bytes - sum(g["size"] for g in got) < info.file_size:
                continue
            data = z.read(info)
            if data[:2] != b"MZ":
                continue
            name = f"{Path(wheel).name.split('-')[0]}__{Path(info.filename).name}"
            p = out_dir / name
            n = 1
            while p.exists():
                p = out_dir / f"{p.stem}_{n}{p.suffix}"
                n += 1
            p.write_bytes(data)
            got.append({"file": p.name, "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                        "wheel": Path(wheel).name, "member": info.filename})
    return got


def cmd_fetch(args):
    out = Path(args.out)
    pe_dir = out / "pe"
    pe_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = out / "manifest.json"
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else []
    have = {m["wheel"] for m in manifest}
    budget = int(args.max_mb * 1024 * 1024) - sum(m["size"] for m in manifest)
    for pkg in PACKAGES:
        if budget <= 0:
            print("corpus size budget reached")
            break
        with tempfile.TemporaryDirectory() as tmp:
            if not pip_download(pkg, tmp):
                print(f"  skip {pkg}: no win_amd64 wheel / download failed")
                continue
            for w in sorted(Path(tmp).glob("*.whl")):
                if w.name in have:
                    continue
                got = extract_pes(w, pe_dir, budget)
                budget -= sum(g["size"] for g in got)
                manifest += got
                have.add(w.name)
                print(f"  {pkg}: +{len(got)} PE files ({sum(g['size'] for g in got) / 1e6:.1f} MB)")
        manifest_path.write_text(json.dumps(manifest, indent=1))
    return cmd_stats(args)


def _scrub_section_names(data, mode):
    """Adversarial variant: overwrite the 8-byte section names so the packer name signature is gone.
    mode 'blank' -> all zero bytes; 'mimic' -> ordinary-looking .text/.data/.rsrc names."""
    import struct
    if data[:2] != b"MZ":
        return data
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    nsec = struct.unpack_from("<H", data, pe + 6)[0]
    opt = struct.unpack_from("<H", data, pe + 20)[0]
    st = pe + 24 + opt
    out = bytearray(data)
    mimic = [b".text\0\0\0", b".data\0\0\0", b".rsrc\0\0\0", b".rdata\0\0"]
    for i in range(nsec):
        o = st + i * 40
        out[o:o + 8] = b"\0" * 8 if mode == "blank" else mimic[i % len(mimic)]
    return bytes(out)


def cmd_pack(args):
    """Real packed fixtures: genuine UPX output for real corpus binaries (+ name-scrubbed adversarial variants).
    Fixtures UPX itself rejects are skipped and counted - never replaced by hand-made ones here."""
    import shutil
    if not shutil.which("upx"):
        print("upx not found (apt install upx-ucl)")
        return 1
    src = Path(args.out) / "pe"
    dst = Path(args.out) / "packed"
    dst.mkdir(parents=True, exist_ok=True)
    cands = sorted(p for p in src.iterdir() if p.is_file() and 20_000 < p.stat().st_size < 4_000_000)
    seen_prefix, picked = set(), []
    for p in cands:  # spread over packages
        pre = p.name.split("__")[0].split("_")[0]
        if pre not in seen_prefix:
            seen_prefix.add(pre)
            picked.append(p)
    made = skipped = 0
    for p in picked:
        if made >= args.count:
            break
        tgt = dst / f"upx_{p.name}"
        r = subprocess.run(["upx", "--best", "--force", "-q", "-o", str(tgt), str(p)], capture_output=True, text=True)
        if r.returncode != 0 or not tgt.exists():
            skipped += 1
            tgt.unlink(missing_ok=True)
            continue
        made += 1
        data = tgt.read_bytes()
        (dst / f"upxblank_{p.name}").write_bytes(_scrub_section_names(data, "blank"))
        (dst / f"upxmimic_{p.name}").write_bytes(_scrub_section_names(data, "mimic"))
    print(f"{made} files packed with UPX ({made * 3} fixtures incl. name-scrubbed variants), {skipped} rejected by UPX")
    return 0 if made else 1


def cmd_stats(args):
    mp = Path(args.out) / "manifest.json"
    if not mp.exists():
        print("no corpus yet - run `fetch`")
        return 1
    m = json.loads(mp.read_text())
    sizes = sorted(x["size"] for x in m)
    print(f"{len(m)} PE files, {sum(sizes) / 1e6:.0f} MB, median {sizes[len(sizes) // 2] / 1e3:.0f} kB, largest {sizes[-1] / 1e6:.1f} MB, "
          f"{len({x['wheel'] for x in m})} wheels")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=str(ROOT / "reverse_corpus"))
    sub = ap.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("--max-mb", type=float, default=700)
    f.set_defaults(fn=cmd_fetch)
    k = sub.add_parser("pack")
    k.add_argument("--count", type=int, default=24)
    k.set_defaults(fn=cmd_pack)
    sub.add_parser("stats").set_defaults(fn=cmd_stats)
    args = ap.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
