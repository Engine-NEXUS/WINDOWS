# Reverse-engineering source into architecture — tools and how to measure them

Research date 2026-11-27 (primary sources). Not fetched: semgrep, pydeps, pyreverse, jQAssistant, structurizr, gitdiagram, repomix, tree-sitter-graph, and the MoJoFM/ARI/A2A definitions — treat those as **unverified**.

| Project | Stars | License | Method | Notes |
|---|---|---|---|---|
| [sopaco/deepwiki-rs (Litho)](https://github.com/sopaco/deepwiki-rs) | 3.2k | MIT | parse + LLM agents (4 stages incl. Mermaid verification/repair) | C4-style docs; page says Litho "has evolved into Terrain"; no eval numbers |
| [AsyncFuncAI/deepwiki-open](https://github.com/AsyncFuncAI/deepwiki-open) | 18.5k | MIT | LLM-centric wiki+diagrams | no eval numbers |
| CodeWiki ([arXiv 2510.24428](https://arxiv.org/html/2510.24428v1)) | — | — | tree-sitter AST → `depends_on` graph → topological sort → recursive LLM agents | CodeWikiBench (21 repos, rubrics from official docs): **68.79 %** (Sonnet 4) vs DeepWiki 64.06 %; C/C++ 53.24 % vs 79.14 % for high-level languages. Measures *documentation* quality, not edge correctness |
| [code2flow](https://github.com/scottrogowski/code2flow) | 4.6k | MIT | static AST heuristics | "pretty good estimate"; `--upstream/downstream-depth` is the nearest built-in blast radius; no accuracy eval |
| [tree-sitter](https://github.com/tree-sitter/tree-sitter) | 27.6k | MIT | parser | already a NEXUS dependency |
| [stack-graphs](https://github.com/github/stack-graphs) | 876 | Apache/MIT | name-resolution path-finding | **archived Sep 2025** |
| [SCIP](https://github.com/scip-code/scip) | 906 | Apache-2.0 | symbol index via per-language indexers | find-references/implementations; no transitive closure |
| [CodeQL](https://github.com/github/codeql) | 10.4k | MIT (queries) | code database | **CLI licensed separately** — closed-source use needs a commercial licence |
| [dependency-cruiser](https://github.com/sverweij/dependency-cruiser) | 7.5k | MIT | static module graph + rules | per-edge rule violations; known-violations file |
| [madge](https://github.com/pahen/madge) | 10.2k | MIT | static module graph | `--circular` returns explicit cycles; `.obj()` adjacency list |

## Findings
- **Explainable paths are rare**: only madge `--circular` and dependency-cruiser violations return inspectable paths/edges; the LLM wiki tools return prose and diagrams, not verifiable paths.
- **No fetched tool documents transitive reverse reachability** ("what breaks if I change X") nor validates its edges against ground truth; compute it yourself from an edge list (invert + BFS) — which is what NEXUS's petgraph-based Architecture Mapper already does.
- **No tool publishes precision/recall for its dependency edges**; CodeWiki evaluates documentation, not edges.

## Ground truth that can be used to measure quality
Runtime import traces vs static edges (log real import/require during tests → edge precision/recall) · madge test fixtures (small known graphs → exact edge-set match / F1) · dependency-cruiser known-violations (precision/recall) · code2flow `make_expected.py` outputs (call-edge precision/recall + ambiguous-edge share) · SCIP indexers vs compiler output (e.g. `tsc --listFiles`) on sampled symbols · maintainer-declared module structure for CodeWiki's 21 repos (needs MoJoFM/ARI definitions verified first) · CodeWikiBench rubrics for *documentation* quality only.
