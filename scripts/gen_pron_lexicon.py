#!/usr/bin/env python3
"""Builds src-tauri/src/sound_match_cmudict.txt, the compact pronunciation table used by sound_match.rs.

    python scripts/gen_pron_lexicon.py path/to/cmudict.dict

Input: the CMU Pronouncing Dictionary (BSD-style licence, Carnegie Mellon University; see THIRD_PARTY_NOTICES.md).
Output: one line per word, sorted, "word<TAB>codes", first pronunciation only, stress removed, one printable character per phoneme
(chr(48 + index in PHONEMES)). Words with anything other than a-z and apostrophe are skipped. About 2 MB for 126,000 words.
The phoneme order below must match PHONEMES in sound_match.rs (a test checks it).
"""
import re
import sys

PHONEMES = ["AA", "AE", "AH", "AO", "AW", "AY", "B", "CH", "D", "DH", "EH", "ER", "EY", "F", "G", "HH", "IH", "IY", "JH", "K", "L", "M", "N", "NG",
            "OW", "OY", "P", "R", "S", "SH", "T", "TH", "UH", "UW", "V", "W", "Y", "Z", "ZH"]


def main(src, dst):
    best = {}
    for line in open(src, encoding="latin-1"):
        if line.startswith(";;;") or not line.strip():
            continue
        word, *ph = line.split("#")[0].split()
        if re.search(r"\(\d+\)$", word):  # alternate pronunciation: keep only the first
            continue
        word = word.lower()
        if not re.fullmatch(r"[a-z']+", word):
            continue
        codes = "".join(chr(48 + PHONEMES.index(re.sub(r"\d", "", p))) for p in ph)
        best.setdefault(word, codes)
    with open(dst, "w", encoding="ascii", newline="\n") as f:
        for w in sorted(best):
            f.write(f"{w}\t{best[w]}\n")
    print(len(best), "words written to", dst)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else "src-tauri/src/sound_match_cmudict.txt")
