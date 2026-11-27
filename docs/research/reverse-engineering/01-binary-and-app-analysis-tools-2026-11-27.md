# Binary and application analysis tools — open-source landscape

Research date 2026-11-27. Star counts as displayed on the GitHub pages; licence as shown; "not shown" = the fetched page did not show it. Releases pages were only fetched for capa and Ghidra (no last-release dates for the rest). The capa rule-format page was truncated (tail model-summarised) and the pefile wiki fetch returned the repo front page (pefile attribute names unverified).

| Tool | Stars | License | Windows | CLI / scriptable | Notes / limitation |
|---|---|---|---|---|---|
| [Ghidra](https://github.com/NationalSecurityAgency/ghidra) | 101.5k | Apache-2.0 (sidebar; repo also has an unexplained GPL/ folder) | yes | automated mode, PyGhidra | needs JDK 25 64-bit; page warns of vulnerabilities in some versions; size/RAM not stated |
| [rizin](https://github.com/rizinorg/rizin) | 4.3k | LGPL-3.0 / GPL-3.0 | yes | `rizin`, `rz-bin`, rzpipe | radare2 fork; copyleft |
| [Cutter](https://github.com/rizinorg/cutter) | 20.3k | GPL-3.0 | yes | GUI only | copyleft |
| [radare2](https://github.com/radareorg/radare2) | 25.5k | LGPLv3 | yes | `r2`, r2pipe | plugin licences vary |
| [capa](https://github.com/mandiant/capa) | 6.4k | Apache-2.0 | yes (`capa.exe` standalone) | CLI + `pip install flare-capa` | capability detection, ATT&CK/MBC; "cannot handle obfuscation well using static analysis"; rules are a separate download |
| [Detect It Easy](https://github.com/horsicq/Detect-It-Easy) | 12.0k | MIT | yes | `diec` batch CLI | file type / packer / compiler ID; "not an antivirus" — heuristics are leads; JSON mode not stated |
| [ImHex](https://github.com/WerWolv/ImHex) | 56.1k | GPL-2.0 (core), LGPL-2.1 parts | yes | GUI only | ~50 MiB RAM baseline; needs OpenGL 3.0 |
| [YARA](https://github.com/VirusTotal/yara) | 10.1k | BSD-3-Clause | yes | `yara`, yara-python | "maintenance mode"; README points to YARA-X |
| [Frida](https://github.com/frida/frida) | 23.0k | not stated | not named | frida-tools | dynamic instrumentation (executes the target) |
| [x64dbg](https://github.com/x64dbg/x64dbg) | 50.4k | not stated | yes | plugins | GUI debugger |
| [ILSpy](https://github.com/icsharpcode/ILSpy) | 27.2k | MIT | yes | `ilspycmd` (`--json` only with `--dump-table/--dump-pdb`) | .NET only; win-x64 Native AOT build needs no runtime |
| [dnSpyEx](https://github.com/dnSpyEx/dnSpy) | 11.6k | GPL-3.0 | not stated | API / C# interactive | copyleft |
| [jadx](https://github.com/skylot/jadx) | 52.0k | Apache-2.0 | yes | `jadx --output-format json` | Android DEX/APK; "can't decompile 100% of the code" |
| [AssetStudio](https://github.com/Perfare/AssetStudio) | 15.6k | MIT | .NET FW | GUI | **archived Jan 2023**; Unity 3.4–2022.1 |
| [asar](https://github.com/electron/asar) | 2.9k | MIT | not stated | `list`, `extract` | archive format, not a decompiler |
| [LIEF](https://github.com/lief-project/LIEF) | 5.7k | Apache-2.0 | parses PE | library (C++/Python/Rust) | imports, sections, resources, rich header, Authenticode (`verify_signature`), CodeView/PDB; .NET/CLR headers not stated |
| [pefile](https://github.com/erocarrera/pefile) | 2.1k | MIT | yes | Python module, no deps | headers, sections, resource strings, PEiD packer signatures, warnings for malformed values; does **not** verify Authenticode |
| [angr](https://github.com/angr/angr) | 9.4k | BSD-2-Clause | not stated | Python | heavy |
| [RetDec](https://github.com/avast/retdec) | 8.7k | MIT (+zlib, third-party) | yes | `retdec-decompiler` | "limited maintenance"; ~5–6 GB installed |

## capa in detail
Rules are YAML (`rule: meta: / features:`). Meta: `name`, `namespace`, `scopes`, `att&ck` ("Discovery::Query Registry [T1012]"), `mbc`, `examples` (the linter checks each rule fires on its examples). Features form an `and/or/not/N-or-more/optional` tree over leaves such as `api`, `string`, `substring`, `import`, `export`, `section`, `characteristic`, `format`, `os`, `arch`, and `match` (reference another rule). Static backends: vivisect, Ghidra, IDA; dynamic: CAPE, DRAKVUF, VMRay reports. The **rules are the valuable, reusable idea**: small declarative capability signatures with ATT&CK mapping and a self-test.

## Fit for a free, BYOK, Windows voice assistant (~700 MB RAM)
1. **LIEF / pefile** — in-process PE metadata, permissive licences.
2. **Detect It Easy (`diec`)** — shell out for packer/compiler identification.
3. **capa.exe** — shell out for capability report; weak on packed samples.
4. **YARA** — tiny permissive pattern matcher (maintenance mode; YARA-X is the successor).
5. **ilspycmd** — .NET assemblies only.
6. **Ghidra / radare2 / rizin** — only as an *optional, user-installed* backend that NEXUS shells out to (JDK 25 + large footprint conflicts with the RAM budget; copyleft rules apply to bundling, not to launching a user-installed tool — confirm with counsel).

Copyleft in this set: rizin, Cutter, radare2, ImHex, dnSpyEx (none AGPL). Licence not stated on the page: Frida, x64dbg — check before bundling.
