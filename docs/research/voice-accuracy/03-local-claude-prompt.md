# Prompt for the local Claude (Windows laptop) - run the voice-accuracy loop and verify it

Paste everything inside the fence into Claude Code running in your local clone of `Engine-NEXUS/WINDOWS`, on branch `claude/nexus-dexter-comparison-1j45rz` (PR #34). All research is already done and written down; this prompt is about **executing and verifying**, not researching.

```text
You are working in my local clone of Engine-NEXUS/WINDOWS (Windows, PowerShell), branch
claude/nexus-dexter-comparison-1j45rz (PR #34). DO NOT MERGE anything and do not touch main.

The research is DONE. Do NOT web-search, do NOT re-audit the codebase from scratch, do NOT redesign anything.
Read ONLY these files (in this order) and trust them; open source files only when a command fails:
  1. AGENTS.md  - the top two sections ("Voice Accuracy Phase E" and "Voice Accuracy Measurement Harness")
  2. docs/changes/91-label-audit-reusable-data-tiered-speech-check.md
  3. docs/research/voice-accuracy/02-runbook-record-score-fix-retrain.md   (the procedure; "Fastest path" first)
  4. docs/research/voice-accuracy/01-voice-accuracy-audit-and-plan-2026-10-20.md  (skim: targets and star thresholds)
  5. docs/wake-word/22-hundred-combo-review.md  (only for the "43% with the speech check" background)

GOAL: run the improvement loop on MY real data and report, in numbers, whether the new wake-word model is
better than the shipped one - without poisoning the data and without claiming anything unmeasured.

STEP 0 - set up (stop and tell me if any item fails; do not work around it silently)
  - git fetch; git checkout claude/nexus-dexter-comparison-1j45rz; git pull
  - pip install numpy scipy onnxruntime sounddevice soundfile piper-tts torch onnx
  - Rust toolchain present (cargo --version). GROQ_API_KEY is optional (only for --stage2 / STT scoring).
  - Run the unit suites and report pass counts (they must pass before you touch data):
      python -m unittest scripts/test_voice_eval_tools.py scripts/test_wake_train_lib.py scripts/test_wake_label_audit.py scripts/test_wake_data_fetch.py scripts/test_wake_data_synth.py scripts/test_wake_score.py scripts/test_wake_improve.py
      cd src-tauri; cargo test --lib -- --test-threads=1      (expect ~973 passed)

STEP 1 - check what data exists. Report counts of wake_word_data/positive, negative, background and whether
  eval_set/manifest.jsonl exists. If wake_word_data/positive has no real recordings of me, STOP and tell me to run
  `nexus wake record 300` (I say "nexus", "hey nexus", "hi nexus", "okay nexus"). If eval_set/ is missing, tell me to run
  runbook section 1 (me + 2-3 other people; this is the only unbiased test) - the loop still runs without it but the
  verdict will be weaker; say so.

STEP 2 - run the loop:  nexus wake improve
  It scores the shipped model, fetches public data (permissive licences only), makes synthetic voices, audits labels,
  and then STOPS with exit code 3. That stop is expected: it needs ME to listen to the flagged clips
  (`nexus wake audit review`). Do NOT use --allow-unreviewed, do NOT answer the review prompts yourself, and do NOT
  move/relabel clips by script: a negative that sounds like "nexus" poisons the model and only my ears decide.
  Tell me how many clips are flagged and wait. After I finish, run `nexus wake improve` again to continue.
  If a download is blocked or Piper voices fail, use --skip-fetch / --skip-synth for that part, say so, and continue.

STEP 3 - verify, do not assume. After the run:
  - Open docs/research/voice-accuracy/improvement-report.md, scorecard-before.md, scorecard-after.md and quote the table.
  - Check each of these and answer yes/no with evidence:
      a) the audit queue was fully reviewed (nexus wake audit status exits 0)
      b) the candidate was scored on audio it did NOT train on (eval_set, sealed takes, public held-out hours)
      c) no row in the report is based on fewer than 20 clips or less than 1 hour of held-out audio, except those marked "too little data"
      d) src-tauri/resources/oww/nexus.onnx is unchanged (git status) unless I asked to promote
  - If the verdict is IMPROVED, run:  nexus wake threshold  (needs a --sweep file; see runbook section 6) and
    nexus wake verify  to choose the speech-check policy (always / tiered / off) from data. Show me the tables and
    recommendation; do not change settings.json defaults yourself.
  - Only if I say "promote": nexus wake improve --promote, then python scripts/wake_data_foundation.py fingerprint-models,
    then commit that on the same branch (no merge).

STEP 4 - report back in this exact shape, short:
  1. Verdict (IMPROVED / SAME / REGRESSED / NO_DATA) and the before -> after table for: each wake phrase recall,
     other people, noisy/far/whisper, soundalike false alarms, false alarms per hour (with hours of audio), wake speed.
  2. What was NOT measurable and why (missing recordings, too few clips, no held-out hours, no GROQ key).
  3. Anything that failed or was skipped, with the exact command and error.
  4. What I should do next (record more / review more clips / promote / nothing).

RULES
  - Never invent or estimate a number. "not measured" is a valid answer.
  - Never train on eval_set/ or on the sealed owner takes. Never delete my recordings (the tools quarantine, not delete).
  - Never skip or loosen a test, gate or the audit to get a better-looking result.
  - Do not merge, rebase over, or resolve the PR's conflicts with main - I will do that myself.
  - If something in the docs contradicts the code, tell me; do not silently pick one.
```

## Notes for you
- The only step that cannot be automated is the by-ear review (`nexus wake audit review`) - it is what protects the model from mislabeled soundalikes.
- Expect the first full run to download several GB (LibriSpeech, MUSAN, Speech Commands, RIRs) and to take a while; `--skip-fetch --skip-synth` gives a fast first pass on your own recordings only.
- The verdict is only as good as the test audio: record the held-out set (runbook section 1) for a trustworthy number.
