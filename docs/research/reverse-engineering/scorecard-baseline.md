# NEXUS Reverse Lab scorecard

**Weighted average: 0.00 / 5** - NOT YET (needs avg >= 4.5, no row < 4, all gates green)

| Row | Weight | Stars | Evidence |
|---|---|---|---|
| PE parse fidelity vs pefile + LIEF | 20 | ☆☆☆☆☆ | not measured (needs >= 200 real PE files and >= 2000 compared fields) |
| Malformed-input robustness | 15 | ☆☆☆☆☆ | not measured (needs >= 2000 mutants) |
| Capability detection | 15 | ☆☆☆☆☆ | not measured (needs >= 50 recall cases, >= 100 benign files, >= 1 rule) |
| Packing / obfuscation honesty | 10 | ☆☆☆☆☆ | not measured (needs >= 20 packed fixtures, >= 100 clean files) |
| HAR -> OpenAPI accuracy | 15 | ☆☆☆☆☆ | not measured (needs >= 40 ground-truth endpoints) |
| Evidence-bound explanation | 10 | ☆☆☆☆☆ | not measured (needs >= 200 adversarial LLM outputs) |
| Speed and footprint | 5 | ☆☆☆☆☆ | not measured (needs >= 30 timed files) |
| Voice / UX and non-regression | 10 | ☆☆☆☆☆ | not measured (needs >= 40 phrases incl. near-misses) |

| Hard gate | Status |
|---|---|
| target is never executed or loaded | not verified |
| HAR secrets never reach output, logs or LLM prompts | not verified |
| binary / HAR never uploaded (capped redacted summary only) | not verified |
| authorization banner on every report | not verified |
| refusal line for crack / keygen / licence-bypass / exploit requests | not verified |
