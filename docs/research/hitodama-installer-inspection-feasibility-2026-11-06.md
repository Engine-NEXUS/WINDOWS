# Hitodama installer inspection: access and technical limits

Date: 2026-11-06. Status: official public pages inspected; no installer obtained or executed. No NEXUS application source, build, runtime or settings changes.

## Findings

The original request asks whether an installer can provide the source code of a closed-source application. An installer normally distributes compiled application files and resources. Package inspection can reveal architecture and dependencies; it does not normally recover the original repository, comments, tests, training data or exact build setup. Some runtimes may include readable scripts; native binaries require different analysis, and recovered code is not automatically licensed for reuse.

Public links on [the official homepage](https://hitodama.in/) lead to purchase and [account access](https://hitodama.in/account/), not an anonymous installer URL. The account page requests the purchase email and sign-in code. No purchase, sign-in, code request, private endpoint access or download restriction bypass was attempted. A bounded filename check in the user's Downloads folder found no file matching Hitodama/Hitodoma. This does not establish absence elsewhere on the device.

The web research fetcher returned 403 for these pages, but an ordinary HTTP request to their public URLs returned 200. The earlier fetch failure therefore did not establish that public documentation itself required authentication. HTML was parsed as text; no website JavaScript or application executable was run.

The [licence terms](https://hitodama.in/legal/terms/) restrict redistribution, licensing/activation workarounds and decompilation/reverse engineering except where law expressly permits it. Those are the published terms, not a determination about a particular statutory exception. Deeper code-level analysis should be scoped with author permission or an applicable right established for the intended use. No protected implementation is copied into NEXUS by this investigation.

## New desktop-control evidence

The [requirements page](https://hitodama.in/requirements/) expressly describes browser control as depending on Windows accessibility and acknowledges unavailable pages/controls. It also identifies Windows secure-surface limits and says riskier terminal/raw desktop automation tools are disabled by default. These are vendor statements, not binary-verified behavior.

This narrows the comparison: Hitodama does not publicly promise unlimited control of every interface. Windows accessibility is a shared foundation with NEXUS. Its exact target ranking, outcome checks, fallback implementation and error rates still cannot be established from these statements.

## Proposed inspection sequence when a legitimate installer is available

| Step | What can be established | Limits |
|---|---|---|
| Preserve the original | File size, SHA-256, acquisition source and timestamp | A hash identifies bytes, not trustworthiness |
| Read metadata and signature | File version, publisher/signing status and PE metadata where present | Publisher signature does not prove desktop-control accuracy |
| List package contents without launching it | Installer format, bundled runtimes, DLLs, resources and dependency notices | Unpack/list with a trusted tool; do not run extracted helper programs |
| Review permitted artifacts | Dependency licences, documented configuration/schema, assets or shipped source where authorized | Do not treat readable proprietary code as reusable open source |
| Map findings to NEXUS | Distinguish verified dependency evidence from hypotheses about architecture | A DLL/import/string alone does not establish an active execution route |
| Compare behavior separately | Same-device supported-task completion and failure recovery | Requires an authorized installation/device test; static analysis cannot measure it |

No installer, extracted binary, account cookie, licence key or proprietary source should be added to the NEXUS repository. Keep any acquired artifact outside tracked project files. Any future runtime experiment is separate from this static-inspection request and must respect the owner-operated runtime/build restriction.

## Current next dependency

An official installer file or authorized download access is needed before package analysis can begin. Source-code access from the developer would be more direct for a code audit. Without either, continue with public documentation, vendor clarifications and a matched black-box task benchmark rather than inventing implementation details.
