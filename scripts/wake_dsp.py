"""Wake-word front-end parameters, shared with the Rust runtime.

The numbers live in src-tauri/resources/oww/dsp_spec.json (read by
src-tauri/src/wake_dsp.rs too). The trainer feeds the classifier with the
'aligned' chain, so serving with wakeDspProfile='aligned' matches training.
Standard library only (numpy optional for the helpers below).
"""

import json
from pathlib import Path

SPEC_PATH = Path(__file__).resolve().parent.parent / "src-tauri" / "resources" / "oww" / "dsp_spec.json"


def load_spec(path=SPEC_PATH):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def aligned_params(spec=None):
    """highpass_hz, silence_rms, target_rms, max_gain, floor_gain of the trainer chain."""
    return dict((spec or load_spec())["aligned"])


def agc_gain(rms, p):
    """Mirror of wake_dsp::agc_gain: lift quiet chunks toward target_rms (capped),
    never less than floor_gain."""
    if rms < p["target_rms"]:
        return max(min(p["target_rms"] / max(rms, 1e-9), p["max_gain"]), p["floor_gain"])
    return p["floor_gain"]
