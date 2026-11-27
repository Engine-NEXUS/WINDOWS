# Prompt for the local Claude (Windows laptop) - verify Reverse Lab on real Windows

Paste the fenced block into Claude Code in your local clone of `Engine-NEXUS/WINDOWS`, branch `claude/nexus-dexter-comparison-1j45rz` (PR #34, **do not merge**). Everything below was built and measured on Linux; these are the checks that need Windows.

```text
You are in my local clone of Engine-NEXUS/WINDOWS (Windows, PowerShell), branch claude/nexus-dexter-comparison-1j45rz.
DO NOT MERGE and do not touch main. DO NOT research or redesign: read docs/features/91-reverse-lab.md and
docs/changes/92-reverse-lab-measured-reverse-engineering-feature.md only, then run and report.

STEP 0  git pull; cargo --version; python --version.
        cd src-tauri\reverse-core; cargo test --lib            (expect 79 passed)
        cd ..; cargo test --lib -- --test-threads=1            (expect ~990 passed; report any failure verbatim)

STEP 1  Real corpus on real Windows binaries (not the Linux wheel corpus):
        python scripts/reverse_corpus.py fetch ; (optional) winget install upx ; python scripts/reverse_corpus.py pack
        nexus reverse score
        Report the scorecard table verbatim. Do NOT edit thresholds or rules to make a row pass; if a row drops, say which.
        Also run:  nexus reverse analyse C:\Windows\System32\notepad.exe   and  ... \cmd.exe  and one .NET exe.
        Tell me whether any of the three crash, take > 2 s, or show a high-severity finding (that would be a false positive).

STEP 2  Live checks (start the app with `nexus start`; do NOT use a bare cargo build; build with --features custom-protocol).
        a) Focus Notepad, say "NEXUS, what does this app do". Expect: the ack "Analysing it statically, sir. Nothing will be run.",
           a sidebar report for notepad.exe with the authorisation banner, then one spoken summary. If it analysed NEXUS itself
           or said it couldn't find the app, that is the foreground-window path (target.rs foreground_exe_path) - report the log lines.
        b) Download any small .exe/.msi, say "analyse the file I just downloaded". Expect that file.
        c) Save a HAR from the browser dev tools of a site you own/use, say "turn the latest har into an API spec".
           Expect endpoints + a saved file under %APPDATA%\com.nexus.assistant\reverse\*.openapi.yaml. Open the YAML and
           confirm there is NO token, cookie, email or example value in it (search for the first characters of your session token).
        d) Say "crack this app" - expect only the refusal line. Say "analyse repo servx" - expect the OLD behaviour (Architect), not Reverse Lab.
        e) With a Groq key set and reverseLabExplain on, repeat (a). Then count in the report: statements shown vs the line
           "N statement(s) from the language model were discarded". Report both numbers.

STEP 3  REAL-MODEL measurement (the only part that was synthetic): with your Groq key in settings, run the report for 10 different
        programs from C:\Windows\System32 and C:\Program Files with explain ON. For every kept statement, check by eye against
        the evidence IDs it cites. Report: statements kept, statements that were wrong or unsupported anyway (the validator's
        misses), and statements discarded. Do not tune the validator; just report.

Report: what passed, what failed with exact output, and anything that surprised you. Keep the PR unmerged.
```
