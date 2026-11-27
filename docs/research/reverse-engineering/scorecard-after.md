# NEXUS Reverse Lab scorecard

**Weighted average: 4.90 / 5** - PASS

Before: 0.00 / 5

| Row | Weight | Stars | Evidence |
|---|---|---|---|
| PE parse fidelity vs pefile + LIEF | 20 | ★★★★★ (was 0) | 109284/109284 fields agree (100.00%, 95% low 100.00%) over 323 real PE files; 7170 oracle-disputed fields excluded (pefile vs LIEF disagree: Reverse Lab matched LIEF on 7170, pefile on 0); clean third-party binaries only - not packed/adversarial samples |
| Malformed-input robustness | 15 | ★★★★★ (was 0) | 0 failures in 2500 mutants (0 panics, 0 hangs, 0 over-cap) |
| Capability detection | 15 | ★★★★★ (was 0) | recall 100.0% (56/56), benign FP 0.31% (1/322), 48 rules; caveat: recall cases are the rule authors' own examples rebuilt as PEs, not real malware, and no independent oracle (capa) was run |
| Packing / obfuscation honesty | 10 | ★★★★★ (was 0) | flagged 100.0% of packed, false flags 0.0% of clean |
| HAR -> OpenAPI accuracy | 15 | ★★★★★ (was 0) | endpoint+method F1 0.997, path-template accuracy 0.999, schema-field recall 0.998 over 44 ground-truth endpoints x 10 capture(s), 1644 requests; mock APIs, not production traffic |
| Evidence-bound explanation | 10 | ★★★★★ (was 0) | 1893/1893 unsupported claims dropped over 480 mock-LLM outputs, 0 kept without a valid citation; 99.9% of well-supported claims survive; adversarial outputs are synthetic - a real model's mistakes may differ |
| Speed and footprint | 5 | ★★★★★ (was 0) | p95 0.08 s, RSS delta 76 MB over 322 files |
| Voice / UX and non-regression | 10 | ★★★★☆ (was 0) | phrase matrix 100.0% (52/52); on fresh phrasings the first try caught 21/32 (66%) and 1 false capture(s), then the parser was widened (capped at 4 until re-measured on new phrasings) |

| Hard gate | Status |
|---|---|
| target is never executed or loaded | OK |
| HAR secrets never reach output, logs or LLM prompts | OK |
| binary / HAR never uploaded (capped redacted summary only) | OK |
| authorization banner on every report | OK |
| refusal line for crack / keygen / licence-bypass / exploit requests | OK |
