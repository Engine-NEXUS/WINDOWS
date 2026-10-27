# Frozen P1 + P2 verification — 2026-10-25

Source: 9c3f0320a5c7df359c6a9769e8492ed48baf8fcd. Input hash: 2317997e0df6a01873674f7174b4c4d60a3d67ec327f90c290b2308c501519d8.

The existing readiness worktree contains P1 plus the seven scoped P2 files. The four hand-focus settings lines from the primary checkout were deliberately excluded. Primary branch/index and concurrent edits remain intact. The main-tree 682 frontend count included four unrelated concurrent tests; frozen source passes 678. Worker 145 and Python 60 pass.

The earlier P1-only optimized build was intentionally stopped before completion to assess one combined candidate. It was never launched or verified. Its incomplete build log is retained. The P1+P2 release is compiling; no live result is claimed yet. Detailed current gate status and pending live rows: [machine-readable assessment](windows1-20261025-voice-lock-ui-frozen-assessment.json).

Live checks must use the completed provenance-verified executable. Capture initial native state; check UI draft versus effective state; Save Off and On; reopen; compare another settings window; restore the original setting. Keep profile failure and slow/refused-save simulations explicitly labelled as fault injection; they do not establish real corrupt-profile recovery. Do not delete or overwrite the owner's enrolled profile for an ordinary settings check.

P1's fresh ten-command owner reconciliation, speaker/replay accuracy and the complete feature gate remain pending. Score stays 41% (2.05/5). This verification does not advance the feature queue.
