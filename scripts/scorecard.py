#!/usr/bin/env python3
"""Feature readiness scorecard.

  python scripts/scorecard.py report            rewrite docs/research/feature-readiness-scorecard-<date>.md
  python scripts/scorecard.py gaps              what each feature below 80% still needs
  python scripts/scorecard.py show <text>       one row
  python scripts/scorecard.py set <text> L=70 T=80 [note="..."]   change a row's axes (then run report)

Data lives in docs/testing/scorecard-data.json. B built, T tests, L live-verified, S safety, I industry parity.
The percentages are a rubric estimate, not a measured accuracy.
"""
import json, os, sys, datetime
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATA = os.path.join(ROOT, "docs", "testing", "scorecard-data.json")
import statistics, io

W = dict(B=0.20, T=0.20, L=0.30, S=0.15, I=0.15)

def _row(x):
    return (x["area"], x["feature"], x["tests"], x["B"], x["T"], x["L"], x["S"], x["I"], x["note"])


def load_rows():
    """Rows in the current target. A row with "scope": "out" (and a "scope_reason") is listed separately."""
    d = json.load(open(DATA, encoding="utf-8"))
    return [_row(x) for x in d if x.get("scope") != "out"]


def load_out():
    d = json.load(open(DATA, encoding="utf-8"))
    return [(_row(x), x.get("scope_reason", "")) for x in d if x.get("scope") == "out"]


ROWS = load_rows()
OUT_ROWS = load_out()


def score(r):
    _, _, _, B, T, L, S, I, _ = r
    return B*W['B'] + T*W['T'] + L*W['L'] + S*W['S'] + I*W['I']

def stars(p):
    h = round(p / 10)  # half-stars 0..10
    full, half = divmod(h, 2)
    return "★" * full + ("½" if half else "") + "☆" * (5 - full - half)

def verdict(p):
    if p >= 80: return "Production"
    if p >= 65: return "Beta"
    if p >= 50: return "Alpha"
    return "Prototype"

def report():
    out = io.StringIO()
    w = out.write
    scores = [score(r) for r in ROWS]
    avg = statistics.mean(scores)
    counts = {v: sum(1 for s in scores if verdict(s) == v) for v in ("Production", "Beta", "Alpha", "Prototype")}

    w("# Feature readiness scorecard — NEXUS (2026-10-20)\n\n")
    w("Purpose: one table to cross-check how complete, tested, and proven each feature really is. Built from the repository (test counts, module sizes), `AGENTS.md` verification records (what was and was not run live), and the existing research (`docs/research/ai-assistant-landscape-audit-2026-09-26.md`, `docs/research/deployment/01-...`, `docs/research/wakeword/...`).\n\n")
    w("## How to read the numbers (please read this first)\n\n")
    w("The **% is a rubric estimate I computed from evidence, not a measured accuracy.** Only the figures in the *Measured numbers* section are real measurements. The rubric is fixed so two reviewers get the same answer from the same evidence:\n\n")
    w("| Axis | Weight | 100 means | 0 means |\n|---|---|---|---|\n")
    w("| **B** Built and wired end to end | 20% | reachable by the user in the shipping path, no dead code | not reachable |\n")
    w("| **T** Automated test depth | 20% | many tests, mutation-checked, integration tests | no tests |\n")
    w("| **L** Live-verified on the real app / hardware | 30% | used repeatedly by the owner or run on real hardware | never run outside unit tests |\n")
    w("| **S** Safety and failure handling | 15% | gated, confirms before acting, fails closed, honest on failure | acts blindly |\n")
    w("| **I** Industry parity / measured quality | 15% | at or above published reference numbers | far below / unmeasured |\n\n")
    w("Stars = % / 20 in half steps. Verdict: **Production** >= 80, **Beta** 65-79, **Alpha** 50-64, **Prototype** < 50. Live verification carries the most weight on purpose: the project's own history shows features that passed unit tests and still failed in a real session.\n\n")
    w(f"## Headline\n\n- {len(ROWS)} features scored. Average **{avg:.0f}%**.\n")
    w(f"- Production: **{counts['Production']}**, Beta: **{counts['Beta']}**, Alpha: **{counts['Alpha']}**, Prototype: **{counts['Prototype']}**.\n")
    w("- Repository size: 191 Rust modules, ~134,700 lines, 2,029 Rust test annotations (23 ignored dev benches), 362 frontend test cases in 45 files, 11 Worker test files, 9 Python test files, 176 change records, 180 feature docs, 382 commits.\n")
    w("- The dominant weakness across the whole project is not missing code, it is *unverified in the running app* (see the L column).\n\n")

    areas = []
    for r in ROWS:
        if r[0] not in areas:
            areas.append(r[0])

    w("## Summary by area\n\n| Area | Features | Avg % | Stars | Avg live-verified (L) |\n|---|---|---|---|---|\n")
    for a in areas:
        rs = [r for r in ROWS if r[0] == a]
        s = statistics.mean(score(r) for r in rs)
        l = statistics.mean(r[5] for r in rs)
        w(f"| {a} | {len(rs)} | {s:.0f}% | {stars(s)} | {l:.0f}% |\n")
    w("\n")

    w("## Full table\n\n")
    for a in areas:
        w(f"### {a}\n\n| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |\n|---|---|---|---|---|---|---|---|---|---|---|\n")
        for r in sorted([r for r in ROWS if r[0] == a], key=lambda r: -score(r)):
            s = score(r)
            w(f"| {r[1]} | {r[2]} | {r[3]} | {r[4]} | {r[5]} | {r[6]} | {r[7]} | **{s:.0f}%** | {stars(s)} | {verdict(s)} | {r[8]} |\n")
        w("\n")

    w("## Measured numbers (the only hard measurements)\n\n")
    w("| What | Measured | Reference / target | Source |\n|---|---|---|---|\n")
    w("| Wake word recall, held-out owner session | 63.2% (quiet 3/10, far 15/25) | >=95% | wake P0 replay baseline |\n")
    w("| Wake word recall on training data | 98.6% (self-scored, optimistic) | - | same, shown for contrast |\n")
    w("| Wake false fires on real background | ~12 per hour (0.42 h sample) | <=0.1 per hour | wake P0 |\n")
    w("| Spoken soundalikes that wake it | 10 of 15 | 0 | wake P0 |\n")
    w("| Early parser accuracy, bare command | 43% correct, 12% WRONG action | - | doc 103 P0 baseline (since improved, not re-measured) |\n")
    w("| NLU candidate intent accuracy | 0.9004 | Rhasspy-style pipelines 92.8-94.8% | phase-11 dataset |\n")
    w("| Cloud STT | ~240 ms, WER on the Whisper curve (~4-5% EN) | Wispr Flow ~700 ms | STT research 2026-10-07 |\n")
    w("| Edge TTS first audio | 409 ms median (290 ms warm) | streaming TTS 75-250 ms | change 132 |\n")
    w("| Reply latency end of speech to first audio | ~2.6 s | - | voice plan 136 |\n")
    w("| Directed-speech gate on real noise log | 72 of 96 ignored (75%), ~4 false accepts | - | change 82 (optimistic) |\n")
    w("| Vision click accuracy on real screens | NOT MEASURED | ScreenSpot-v2 ~90% easy, OSWorld-G ~50-54% | deployment paper |\n")
    w("| Idle RAM | 104-269 MB | Jan ~200 MB, LocalAI ~500 MB | landscape audit / doc 131 |\n")
    w("| Hand camera rate on battery | ~15 fps (driver halves it) | 30 fps | hand field test notes |\n\n")

    w("## Industry comparison (same features, nearest reference)\n\n")
    w("| Capability | NEXUS | Nearest reference | Where NEXUS is ahead | Where NEXUS is behind |\n|---|---|---|---|---|\n")
    w("| Wake word | Custom openWakeWord, 63% held-out recall | Alexa / Siri / Google (multi-stage, cloud verify, years of tuning) | Personalised, hardware-invariance tests | Recall and false-wake rate far from product grade |\n")
    w("| Command understanding | Regex > BERT-Mini > Qwen > cloud | Leon, Rhasspy, Home Assistant | More tiers, <1 ms primary path | Accuracy 0.90 vs 0.93-0.95 pipelines |\n")
    w("| Voice latency | 240 ms STT, 409 ms TTS start | Wispr Flow ~700 ms, Pipecat ~500 ms | Faster STT path | Reply ~2.6 s because of cloud LLM |\n")
    w("| Computer control | Keyboard-first Ghost, UIA, vision fallback | Open Interpreter OS mode, OSWorld agents (20-55%) | Safety design (Esc, task abort, protected windows) | Click accuracy unmeasured |\n")
    w("| Memory | Encrypted local store, provenance, egress log | Hermes / Letta designs, ChatGPT memory | Local, auditable, user-editable | Retrieval is lexical + synonyms, not semantic |\n")
    w("| Agent watching | Hooks + verdict engine + unconditional/conditional shutdown | Nothing comparable in open source | Unique feature | Most agent hooks unverified; one session id per agent |\n")
    w("| RAM footprint | 104-269 MB idle | Jan ~200 MB | Lazy loading | Kokoro +368 MB when active |\n")
    w("| Platform reach | Windows | Home Assistant / Open Interpreter cross-platform | Deep Windows integration | macOS/Linux largely unbuilt |\n\n")

    low = sorted(ROWS, key=score)[:12]
    w("## Weakest 12 (where the next effort should go)\n\n| Feature | Score | Main reason |\n|---|---|---|\n")
    for r in low:
        w(f"| {r[1]} | {score(r):.0f}% | {r[8] or 'low live proof'} |\n")
    w("\n")
    if OUT_ROWS:
        w("## Out of the current target (decision recorded)\n\n| Feature | Score | Why it is out |\n|---|---|---|\n")
        for r, why in OUT_ROWS:
            w(f"| {r[1]} | {score(r):.0f}% | {why} |\n")
        w("\n")
    w("## What would move the numbers fastest\n\n")
    w("1. **One rebuilt-app voice walkthrough** of the newest features (tracker, tiling, display-off, commit/PR, overnight record, wake cue, boot hello). It lifts the L axis on about 20 rows at once; no new code needed.\n")
    w("2. **Wake word**: record the >=10 h of ambient audio the plan asks for and run the session-separated replay; the 63% / 12-per-hour baseline is the single biggest product risk.\n")
    w("3. **Vision click accuracy**: run the existing calibration suite on real screens and record the result; today that number does not exist.\n")
    w("4. **Real shutdown rehearsal**: `dry`, then one `on` run with a deliberately failing agent (verifies unconditional shutdown plus the morning briefing).\n")
    w("5. **Live hook runs** for Codex, Devin, Windsurf, Kilo, Copilot (one agent at a time, read the `[WATCH-POWER] agent event` line).\n\n")
    w("## Caveats\n\n- Scores for items I did not re-run today come from the project's own verification notes in `AGENTS.md`; I did not re-verify them live.\n- Test counts are `#[test]` annotations per module, not pass counts; a few suites flake under parallel load (timing tests).\n- Weighting is a judgement. Change the weights in the rubric and the table recomputes the same way; the raw B/T/L/S/I columns are given so you can re-score any row.\n")

    path = os.path.join(ROOT, "docs", "research", "feature-readiness-scorecard-" + datetime.date.today().isoformat() + ".md")
    open(path, "w", encoding="utf-8", newline="\n").write(out.getvalue())
    print("wrote", path, len(ROWS), "rows, avg", round(avg, 1), counts)
    for a in areas:
        rs = [r for r in ROWS if r[0] == a]
        print(a, round(statistics.mean(score(r) for r in rs)))


def gap_lines():
    lines = []
    for r in sorted(ROWS, key=score):
        s = score(r)
        if s >= 80:
            continue
        B, T, L, S, I = r[3:8]
        need = 80.5 - s
        # cheap levers first: B->100, S->90, T->85, then L to 90, then I to 75
        plan = []
        for key, cur, cap, wt in (("B", B, 100, W["B"]), ("S", S, 90, W["S"]), ("T", T, 85, W["T"])):
            if need <= 0:
                break
            if cur < cap:
                gain = min(need, (cap - cur) * wt)
                new = cur + gain / wt
                plan.append(f"{key}->{new:.0f}")
                need -= gain
        if need > 0 and L < 90:
            gain = min(need, (90 - L) * W["L"])
            plan.append(f"L->{L + gain / W['L']:.0f}")
            need -= gain
        if need > 0 and I < 75:
            gain = min(need, (75 - I) * W["I"])
            plan.append(f"I->{I + gain / W['I']:.0f}")
            need -= gain
        lines.append((r[1], s, ", ".join(plan), need > 0.01))
    return lines


def main(argv):
    cmd = argv[1] if len(argv) > 1 else "report"
    if cmd == "gaps":
        for f, s, p, unreachable in gap_lines():
            print(f"{s:5.1f}%  {f}  ->  {p}{'  (still short)' if unreachable else ''}")
    elif cmd == "show":
        q = " ".join(argv[2:]).lower()
        for r in ROWS:
            if q in r[1].lower():
                print(r[1], f"{score(r):.1f}%", stars(score(r)), dict(zip("BTLSI", r[3:8])))
    elif cmd == "set":
        q = argv[2].lower()
        d = json.load(open(DATA, encoding="utf-8"))
        hit = [x for x in d if q in x["feature"].lower()]
        if len(hit) != 1:
            sys.exit(f"{len(hit)} rows match {q!r}")
        for kv in argv[3:]:
            k, v = kv.split("=", 1)
            hit[0][k] = v if k in ("note", "tests") else int(v)
        open(DATA, "w", encoding="utf-8", newline="\n").write(json.dumps(d, indent=1, ensure_ascii=False) + "\n")
        print("updated", hit[0]["feature"])
    elif cmd != "report":
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
    if len(sys.argv) <= 1 or sys.argv[1] == "report":
        report()
