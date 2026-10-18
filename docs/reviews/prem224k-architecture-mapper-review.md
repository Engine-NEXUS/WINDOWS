> **Written 2026-08-29, when the architecture mapper was spec-only.** The mapper has since been built (phase 1 layers, phase 2 import graph, blast radius, cycle detection). Treat the "not implemented" findings below as historical.

# Review: prem224k — Architecture Mapper Feature Spec

**Reviewer:** Devin (automated cross-check)
**Date:** 2026-08-29
**Commit reviewed:** `997afcf` on `main`
**Author:** prem22k (`premsai224k@gmail.com`)
**Files changed:** 1 file added, 586 lines
**File:** `docs/features/16-architecture-mapper.md` (25 KB)

---

## 1. Commit Summary

| Field | Value |
|---|---|
| Commit | `997afcf` |
| Author | prem22k (`premsai224k@gmail.com`) |
| Date | Aug 29, 2026 11:35 IST |
| Type | Documentation only — no code changes |
| Branch | `main` (merged directly, no PR) |

The commit adds a comprehensive feature specification for the **Architecture
Mapper**. It describes a
three-phase progressive architecture for turning an unfamiliar repository into
an explorable map of architecture and dependencies, with impact analysis and
an agentic chat interface, built into NEXUS.

---

## 2. What Was Added

A three-phase progressive architecture:

```
Phase 1 (~8s)    → GitHub API fast map → LLM clusters file tree → ReactFlow diagram
Phase 2 (~60s)   → git clone --depth=1 → Rust import graph (rayon, petgraph) → real edges
Phase 3 (always) → Voice/text agentic chat → reverse BFS impact analysis → explainable paths
```

### Phase 1 — Fast Visual Map
- Reads the OS foreground window title to detect a GitHub repo URL.
- Calls GitHub API (5-8 calls, ~2-4s): repo metadata, full file tree, key
  config files.
- Sends structured data to Cloudflare Worker → LLM (Mistral 24B) clusters the
  file tree into architectural layers.
- Returns typed ReactFlow JSON; the `architect` window opens immediately.

### Phase 2 — Real Dependency Graph (background)
- `git clone --depth=1` to `%APPDATA%\nexus\repos\{owner}-{repo}\`.
- Rust background thread (rayon parallel) walks files, extracts imports via
  regex, builds an adjacency list, inverts it, computes in/out-degree,
  detects cycles (DFS), identifies hub nodes.
- Graph JSON → Worker → LLM enriches the diagram in-place.
- Disk-cached for sub-10ms repeat queries.

### Phase 3 — Agentic Chat (Impact Analysis)
- User clicks a node or asks by voice/text.
- Rust reverse BFS on the inverted graph → affected files + paths.
- LLM narrates the path (A → B → C) and explains production risk.
- ReactFlow animates the blast radius.
- Cursor-style text chat sidebar with `@filename` references.

### New Tauri Window: `architect`
- 1400 × 900, resizable, with decorations.
- Top bar (repo + phase status), ReactFlow canvas, right panel (node detail +
  chat), bottom bar (progress).

### New Rust Commands
- `get_active_repo_url` — reads foreground window title → extracts
  owner/repo.
- `analyze_repo_deep` — background clone → walk → extract → graph → emit.
- `query_impact` — reverse BFS on cached graph, sub-10ms.

### New Rust Crates
- `walkdir 2.5`, `ignore 0.4`, `rayon 1.10`, `petgraph 0.6`.

### Cloudflare Worker Changes
- New `analyze_repo` intent in `keywordFallback()`.
- New `handleAnalyzeRepo()` handler with three sub-modes (phase 1/2/3).
- Reuses existing `getValidGithubToken()`, `SUMMARY_MODEL`,
  `ANALYSIS_MODEL`, `extractText()`.

---

## 3. Strengths

| # | Strength | Detail |
|---|---|---|
| 1 | Matches problem statement well | Covers all 12 evaluation criteria with a coverage matrix (lines 539-553). |
| 2 | Three-phase progressive design is smart | User gets value in 8s (Phase 1), deepens silently (Phase 2), then interactive (Phase 3). Better UX than any single-shot approach. |
| 3 | Real import graph, not LLM guessing | Phase 2 uses Rust + rayon + petgraph to parse actual `import`/`require`/`use` statements. LLM explains, doesn't discover. |
| 4 | Explainable impact analysis | Reverse BFS with path reconstruction (A→B→C), not just a file list. Matches the "must show dependency paths" constraint. |
| 5 | Voice-first differentiator | "NEXUS, analyze this repo" triggered from the active browser tab. No competitor has this. |
| 6 | Reuses existing NEXUS infrastructure | Cloudflare Worker, D1 tokens, Tauri windows, sidebar pattern, wake word — all existing. |
| 7 | Honest scope limitations section | Admits file-level only (no function-level), single-repo only, no runtime deps. Good for credibility with judges. |
| 8 | Smart sampling for large repos | >500 files → prioritize entry points + 3-hop BFS + high-signal files. Addresses scalability. |
| 9 | Disk-cached graph | Sub-10ms impact queries after Phase 2. No API call needed for repeat queries. |
| 10 | Cross-references existing docs | Links to AGENTS.md, 07-app-registry.md, 13-response-sidebar.md. Shows awareness of the existing codebase. |

---

## 4. Concerns and Gaps

| # | Issue | Severity | Detail |
|---|---|---|---|
| 1 | No tree-sitter — regex import extraction only | Medium | The spec uses regex patterns for import extraction (lines 211-218). Regex will miss dynamic imports, re-exports, barrel files, and complex patterns. Tree-sitter is the proven approach for AST-level extraction. The spec acknowledges this as "V2" but it's a real limitation. |
| 2 | No code implemented — spec only | High | This is a 586-line markdown file. Zero Rust, zero TypeScript, zero React. It needs a working demo, not a spec. |
| 3 | `petgraph 0.6` is outdated | Low | Current version is 0.8.x. Minor but shows the spec wasn't verified against actual crate versions. |
| 4 | ReactFlow ported from "Zync" | Medium | Line 422: "Source: `Zync/src/components/zlam/ArchitectureMap.tsx`" — this references an external project that may not exist in this repo. Needs verification. |
| 5 | No function/method-level analysis | Medium | Spec admits file-level only. The problem statement explicitly lists "Function calls, Method calls" as expected relationships. Judges may dock points. |
| 6 | No database entity extraction | Medium | Problem statement lists "Database entities" and "Database interactions" as expected. Spec doesn't address this. |
| 7 | No event/message flow detection | Medium | Problem statement lists "Event publishing and consumption". Spec doesn't address pub/sub or event-driven patterns. |
| 8 | No test coverage analysis | Low | Problem statement lists "Test coverage". Spec mentions test files are skipped, not analyzed. |
| 9 | GitHub-only URL detection | Low | Only detects GitHub repos from browser title. GitLab, Bitbucket, local repos not supported. |
| 10 | No centrality algorithm specified | Low | Spec mentions "centrality" but doesn't specify PageRank, betweenness, or eigenvector. Proven approaches are PageRank + betweenness centrality. |
| 11 | LLM model choices may not be available | Low | Spec references "Mistral 24B" and "GLM-4.7-Flash" on Cloudflare Workers AI. Need to verify these are actually available in the Workers AI catalog. |
| 12 | Merged directly to main without PR | Process | No pull request, no review. For a solo contributor this is acceptable but bypasses CI and review. |

---

## 5. Comparison Against Independent Research

| Aspect | Independent Research Recommendation | prem224k's Spec | Match? |
|---|---|---|---|
| AST parsing | tree-sitter (8+ languages) | Regex patterns (6 languages) | Diverges — regex is weaker |
| Graph engine | petgraph | petgraph | Match |
| Impact analysis | BFS on incoming edges | Reverse BFS on inverted graph | Match |
| Cycle detection | Tarjan SCC | DFS cycle detection | Close enough |
| Centrality | PageRank + betweenness | "centrality" (unspecified) | Vague |
| LLM role | Explain paths, not discover | "LLM explains graph, not discovers" | Match |
| Visualization | cytoscape.js | ReactFlow | Different but valid |
| Voice | NEXUS wake word + STT | "NEXUS, analyze this repo" | Match |
| Code embeddings | CodeBERT / UniXcoder | Not mentioned | Missing |
| Scalability | rayon + ignore crate | rayon + smart sampling | Match |

---

## 6. Criteria Coverage Assessment

| Problem Statement Criterion | prem224k Coverage | Score |
|---|---|---|
| Codebase Understanding | File + layer clustering | 7/10 (no function-level) |
| Relationship Discovery | Import graph (regex) | 6/10 (no DB, events, runtime) |
| Architecture Mapping | ReactFlow layered diagram | 8/10 (good progressive approach) |
| Change Impact Analysis | Reverse BFS + paths | 9/10 (strong) |
| Explainability | LLM narrates paths A→B→C | 9/10 (strong) |
| Actionability | Click node → instant query | 8/10 (good) |
| Technical Quality | Rust + Worker + ReactFlow | 7/10 (spec only, no code) |
| AI Value | LLM explains, doesn't discover | 8/10 (correct philosophy) |
| Efficiency | <10ms cached queries | 8/10 (good) |
| Scalability | Smart sampling, disk cache | 7/10 (no incremental updates) |
| Innovation | Voice from active browser tab | 9/10 (unique) |
| Product Potential | Cursor-style + voice | 8/10 (good) |

**Overall: 86/120 — solid spec, but needs implementation.**

---

## 7. Hard Constraints Check

| Constraint (verbatim from problem statement) | How the spec satisfies it |
|---|---|
| "A folder tree is not architecture" | Phase 2 builds a real import graph. Every edge is a parsed statement — not a directory listing. |
| "LLMs must add value beyond summarizing source" | LLM receives pre-computed graph JSON. It explains and narrates — the graph algorithm discovers, the LLM communicates. |
| "Impact analysis must be explainable — show dependency paths" | The spec shows the PATH (A→B→C→root), animated in the graph and spoken by NEXUS. |
| "Consider scale" | Smart file sampling for large repos, petgraph-based O(V+E) algorithms, disk-cached graph. |
| "Static analysis is not the only approach" | LLM semantic layer adds understanding of intent and risk beyond what static imports show. |

---

## 8. Honest Assessment

The spec is well-written and demonstrates good understanding of the problem.
The three-phase progressive architecture is genuinely clever — immediate
value (Phase 1) → silent deepening (Phase 2) → interactive exploration
(Phase 3) is a better UX than any competitor approach researched.

**However, it is a document, not a product.** Users will want
to see a working demo. The spec needs:

1. **Implementation** — 586 lines of markdown → needs ~2000+ lines of Rust +
   TypeScript + React.
2. **tree-sitter instead of regex** — for proper import extraction. Regex
   will fail on dynamic imports, re-exports, and barrel files.
3. **Database/event relationship detection** — even basic ORM model detection
   would help cover more criteria.
4. **Function-level analysis** — at least for the demo repo, to satisfy the
   "Function calls, Method calls" criterion.

---

## 9. Recommended Next Steps

The spec is a good **blueprint**. To be useful, it needs to be built.
Priority order:

1. **Phase 2 Rust core** (clone + import extraction + petgraph) — this is the
   moat. Without real edges, the demo is just a folder tree.
2. **Phase 1 Worker handler** (GitHub API + LLM clustering) — quick win, gives
   the 8-second first impression.
3. **ReactFlow architect window** — visualization. Port from Zync if it
   exists, otherwise build fresh.
4. **Phase 3 impact analysis + voice integration** — the demo moment. "What
   breaks if I change this?" → animated blast radius + spoken explanation.

---

## 10. Verdict

| Question | Answer |
|---|---|
| Does the spec understand the problem? | Yes |
| Does it cover all criteria? | Mostly — missing DB, events, function-level |
| Is the architecture sound? | Yes — three-phase progressive is smart |
| Is it implemented? | No — documentation only |
| Is it ready for production as-is? | No — needs working code |
| Is it a good starting point? | Yes — strong blueprint for implementation |

**Approval status:** Approved as a planning document. Needs implementation
before it can be evaluated as a product.
