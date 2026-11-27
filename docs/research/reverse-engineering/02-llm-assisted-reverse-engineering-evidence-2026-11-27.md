# LLM-assisted reverse engineering — what the repos and papers actually show

Research date 2026-11-27. Sources are the repos' own pages/READMEs and three papers, read in full by a research agent; every number is the source's own. **Not verified / not found** is stated where a source did not answer. GitHub pages showed commit counts, not last-commit dates, so no activity dates are claimed.

## Repositories

| Repo | Stars | License | How the LLM connects | Needs paid software? | Warnings in README |
|---|---|---|---|---|---|
| [LaurieWired/GhidraMCP](https://github.com/LaurieWired/GhidraMCP) | 12.1k | Apache-2.0 | MCP only (Python bridge ↔ Ghidra plugin on localhost:8080); no key/model setting | No (Ghidra 12.1.4, Java 21 to build; headless via PyGhidra) | none stated |
| [clearbluejar/pyghidra-mcp](https://github.com/clearbluejar/pyghidra-mcp) | 470 | Apache-2.0 | MCP (streamable-http/stdio) | No (Ghidra + Python, or Docker) | beta; search index lags analysis. `rename_variable` *errors on ambiguity instead of guessing* |
| [mrexodia/ida-pro-mcp](https://github.com/mrexodia/ida-pro-mcp) | 13.6k | MIT | MCP only | **Yes — IDA Pro 8.3+; "IDA Free is not supported"** | "LLMs are prone to hallucinations"; "NEVER convert number bases yourself" (use `int_convert` tool); "will not perform well on obfuscated code"; exposes arbitrary-Python `py_eval` |
| [jtsylve/re-mcp](https://github.com/jtsylve/ida-mcp) | 181 | MIT + Apache-2.0 | MCP (stdio / HTTP with bearer token) | IDA backend needs a licence; Ghidra backend does not | scripting tool **off by default**; mutations return old+new values; `readOnlyHint`/`destructiveHint` annotations |
| [albertan017/LLM4Decompile](https://github.com/albertan017/LLM4Decompile) | 7.3k | MIT code; model weights under DeepSeek model licence | local model (HF transformers; example needs CUDA) | No, but GPU practical | x86-64 Linux/GCC only; Ref models need Ghidra output |
| [JusticeRage/Gepetto](https://github.com/JusticeRage/Gepetto) | 3.5k | GPL-3.0 | **BYOK** (Anthropic, OpenAI, Gemini, Groq, Together, Ollama, LM Studio…), no tool-calling | **Yes — IDA 7.6+ with Hex-Rays decompiler** | "may very well get things wrong! Always be critical" |
| [radareorg/r2ai](https://github.com/radareorg/r2ai) | 547 | MIT | BYOK / local; ReAct function-calling over r2 commands | No | none stated |

Not verified: REVA (search returned 0 results), decompiler-explorer/dogbolt (404), Binary Ninja Sidekick, other "ghidra-ai" plugins.

## Papers

**LLM4Decompile** ([arXiv 2403.05286v3](https://arxiv.org/html/2403.05286v3)) — metric: *re-executability* (compiles + passes assertions). HumanEval-Decompile average over O0–O3: GPT-4o 16.01 %, LLM4Decompile-End-6.7B 45.37 %; refining **Ghidra output** (Ref models) 46–64 % vs Ghidra alone 20.12 %. ExeBench (real functions): End-6.7B 17.98 %, GPT-4o 3.78 %. Control-flow flattening drops Ref-6.7B from 0.527 → 0.052 (model-summarised section). C / x86 only; 64 % of End-6.7B HumanEval errors are logic errors.

**DecompileBench** ([arXiv 2505.11340v1](https://arxiv.org/html/2505.11340v1)) — 23,400 functions from 130 OSS-Fuzz programs, 6 classic decompilers vs 6 LLM approaches. Averages (RSR = recompile success, CER = branch-coverage equivalence): **Hex-Rays 0.583 / 0.418**, Ghidra 0.413 / 0.278, RetDec 0.354 / 0.155, GPT-4o (refining Hex-Rays) 0.559 / 0.346, GPT-4o-mini 0.582 / 0.254, LLM4Decompile 0.264 / 0.164. LLMs win on **readability** (LLM-judge Elo: 1581 vs Hex-Rays 1162) but lose on **correctness**; the abstract's "52.2 % lower" is the *top of a 17.2–52.2 % range*, not an average. Failure modes: hallucinated variable inference, fabricated types, invented headers, dropped parameters (ABI breaks). Obfuscation and function-name recovery are not evaluated. Human validation was n = 30.

**SoK: Potentials and Challenges of LLMs for RE** ([arXiv 2509.21821v1](https://arxiv.org/html/2509.21821v1)) — a survey (44 papers + 18 projects), not an experiment. Zero-shot prompting "frequently suffers from hallucination and inconsistent semantics". Recommendations: integrate with established tools rather than replace them; validate by recompilation/ground truth; constrain the model with program analysis; keep humans in the loop; test on obfuscated binaries; pin model versions. It reports **no function-name-recovery accuracy** and no hallucination rate.

## Design lessons (each tied to a source above)
1. **LLM refines classic output, never replaces it** — Ref(Ghidra) 46–64 % vs Ghidra 20 %; Hex-Rays still best on RSR/CER (DecompileBench T3).
2. **Compile success is a weak signal** — GPT-4o-mini ties Hex-Rays on RSR (0.582 vs 0.583) but CER is 0.254 vs 0.418.
3. **Deterministic tools for exact work** — ida-pro-mcp's `int_convert`; pyghidra-mcp errors on ambiguous renames.
4. **Anything that executes code is off by default**; destructive edits need confirmation (re-mcp annotations, `py_eval` risk).
5. **Return old + new value for every mutation** → spoken confirmation and undo become possible (re-mcp).
6. **Detect obfuscation/packing first and warn** (ida-pro-mcp advice; LLM4Decompile's CFF collapse).
7. **BYOK + local models first-class** (Gepetto); LLM4Decompile-class models are a poor fit for a free CPU-only app.
8. **Check names/types the model invents** exist before applying them (DecompileBench App. G).

## Claims NOT supported by the evidence
"LLMs beat decompilers" (readability only) · "52.2 % lower correctness" as an average · any function-name-recovery or hallucination percentage · general obfuscation robustness · LLM4Decompile numbers transfer to real-world binaries (it scores 0.264 RSR vs Ghidra 0.413 on DecompileBench) · stars as quality.
