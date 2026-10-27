# Windows 1.0 readiness assessment — 2026-10-25

Status: **in progress**. Speaker verification remains the active feature; no production-ready score or owner live proof is claimed.

## Source identity

- Original branch/HEAD: `feat/action-kernel` / `fd89438fef88dc8270998048d00d1d73baab8e27`.
- Local source snapshot: `954c57471be0564f69763f73b43e0868f50c74ca`.
- Retained ref: `refs/codex/readiness/windows-1.0-20261025T052218Z`.
- Frozen checkout: `C:\Users\Chitkul Lakshya\.codex\worktrees\windows-readiness\ULTRON`.
- Frozen application/build-input SHA-256: `3087f61760442b846c71baaba710b625cf5ebe961e81fa5d4677c381e150ce5e`.
- The original checkout's pre-run hash was `14c54a161aadd6617143b6dd1dd2126b895042eb894150d439d151e1cd2c7c3d`. Subsequent edits to `memcore/mailtriage.rs` and `memcore/mailwording.rs` invalidated that source association. They were preserved in `D:\ULTRON`; the final assessment was moved to the isolated snapshot above.
- The snapshot includes the pre-existing working-tree edits and new sources. It was made with a temporary Git index; the user's branch, HEAD, working files, and primary index were preserved. Subsequent evidence/documentation edits do not change the assessed application inputs.
- The application fingerprint includes the shipped OWW models and configuration. Python caches are excluded. A regression test verifies both rules.
- The frozen checkout was clean and its fingerprint still matched after the complete test run and before the release build and after candidate startup.
- Toolchain/OS details are saved in [the environment record](windows1-20261025-environment.txt): Rust/Cargo 1.93.1, Node 24.12.0, Python 3.12.7, Windows x64 build 26300; one Cargo job.

## Checks

| Check | Result | Evidence / command |
|---|---|---|
| Complete Rust tests | Pass: 2,522 library + 19 integration; 0 failed, 38 explicitly ignored | `cargo test --tests --no-fail-fast --features custom-protocol,admin-brain -j 1 -- --test-threads=1` with shared dependency cache; [frozen log](windows1-20261025-frozen-rust.log). Includes 24 profile tests and the real-model enrollment/reset regression. |
| Frontend TypeScript and production build | Pass | `npm run build` in `frontend`; 950 modules |
| Frontend tests | 647 pass, 66 files | `npx vitest run --reporter=dot` in `frontend` |
| Worker TypeScript | Pass | `npx tsc --noEmit` in `server/worker` |
| Worker tests | 145 pass, 11 files | `npm test -- --reporter=dot` in `server/worker` |
| Python | 48 pass | `python -m pytest scripts server/memory server/agent server/nlu -q` |
| NLU data/lock validation | Pass | `python server/nlu/data_foundation.py validate`; 512 evaluation rows, 8 registered sources |
| Speaker safety mutations | 12/12 killed in frozen checkout | `python scripts/speaker_mutation_check.py`; production profile/crypto compilation and both baselines pass; [frozen log](windows1-20261025-frozen-mutations.log) |
| Build provenance regression | Pass | `node --test scripts/build-provenance.test.mjs` |
| Windows key-store prerequisite | Pass in frozen binary, two separate processes | `live_keyring_write`, then `live_keyring_read_and_delete`; [frozen log](windows1-20261025-frozen-keyring.log). The reserved entry persisted and was removed. This is not owner/profile live proof. |
| Isolated Smart Turn latency | Pass: 721 ms best of three, budget 5,000 ms | [Frozen benchmark](windows1-20261025-frozen-smart-turn-latency.log); the original timing budget remains enforced separately from correctness. |
| IPC contract | Pass | 236 commands, 175 invoked; baseline shrank to 61 known unused commands and 6 known one-sided events |
| Turn-end / hygiene / window opacity | Pass in frozen checkout | 131 turn-end files; 1,572 tracked hygiene files; 12 CSS surfaces |
| Whitespace | Pass | `git diff --check` |
| Dead-code ratchet | Pass in frozen checkout | 148 known findings, none new; [frozen log](windows1-20261025-frozen-dead-code.log) |
| Fresh release / provenance | Pass: `812a5b3333669e02` | `cargo rustc --release --bin nexus --features custom-protocol,admin-brain -j 1 -- -o <frozen-stage>/nexus.exe`; [build log](windows1-20261025-frozen-release-build.log). Optimized release completed in 40m47s; source, frontend and executable hashes verified before and after startup. [Release identity](windows1-20261025-release.json). |
| Candidate startup | Stage ready; microphone and three wake models detected; warnings retained | PID 13408 at launch. Missing optional command models, skipped Groq model and registry-query error remain recorded in [startup evidence](windows1-20261025-startup-evidence.txt). This is not owner voice/ears/visual proof. |
| Real-device speaker trials | Not run | Owner voice, ears and visual judgment required; no live-score increase |

The mutation run caught removal of the locked voice gate, corrupt-settings hold, settings-save preservation, missing-profile refusal, stale-profile clearing, profile validation, independent voice key, deletion failure reporting, encrypted storage, held-out calibration, authentication failure refusal, and frontend confirmation gate. It used isolated copies of the actual production modules and frontend tests; it did not mutate the shared checkout.

## Earlier failures and superseded runs

- A parallel Rust build hit Windows paging-file error 1455. Subsequent commands use one Cargo job.
- An earlier whole-suite Smart Turn latency assertion measured 8.9 seconds under load. Model correctness remains in the normal suite; the original five-second latency budget is retained as an isolated ignored benchmark and passed when run alone.
- The prior library run passed 2,514 tests with 31 ignored tests. It predates the recovery/model-pipeline fixes and is not the final assessment of this snapshot.
- A later original-tree complete run passed 2,522 library and 19 integration tests (38 ignored), including the real-model enrollment test. Its isolated Smart Turn benchmark passed at 2,308 ms. Concurrent mail-source edits were then detected by the fingerprint check, so those results were not treated as final proof of the frozen snapshot. The [original full log](windows1-20261025-full-rust.log) and [latency output](windows1-20261025-smart-turn-latency.log) are retained.
- Integration/full-suite builds started before the second-pass fixes were stopped as superseded. They do not count as final passes.
- The frontend mutation harness initially failed to decode UTF-8 output on Windows; it was repaired and rerun successfully.
- The second pass found and fixed false-success deletion, an invalid enrollment model pipeline, shared-memory-key coupling, corrupt-profile re-enrollment failure, and settings paths that could silently disable the lock.

Ignored tests that access real hardware, credentials, network services, or research inputs remain explicit separate checks; an ignored test is not a pass. Voice Lock remains off by default and the queue must not advance to installer readiness before the owner live proof.
