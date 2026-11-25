# 5. WhatsApp priority people (read-only)

Scorecard before: Built 85, Tested 65, Live 5, Safety 90, Industry 50 (2.5 stars). Off by default.

## What it is

`memcore/wa.rs` polls the local WhatsApp MCP bridge (45 s while you are present, 5 min while away) through a read-only allowlist and `memcore/people.rs` learns who matters from **counts and timing only** (two-way volume, reply-within-an-hour rate, recency; one-sided senders count a quarter; VIP +30; mute = 0). Alerts name the sender; message text is never used to rank and is shown only in a local sidebar card. Settings `memcoreWhatsapp` and `memcoreWhatsappSpeak`, both default off.

## Verification (what was found)

Baseline: 17 wa and 8 people tests, all pass (a mock bridge that advertises `mark_read` and similar tools sees only `tools/list`, `list_chats`, `list_messages`; `mcp_client::call_tool` refuses the write tools for every caller).

| # | Finding | Evidence | Severity |
|---|---|---|---|
| 1 | The chat card ended with "the sender will not see blue ticks from this". Nothing proves that: whether the **bridge's own read calls** send receipts or presence is documented as unverified (`docs/testing/whatsapp-read-receipt-test.md`, `wa.rs` module header) | `wa.rs:521` | Medium, a promise the code cannot keep |
| 2 | The settings panel said "NEXUS never marks chats as read (no blue ticks)" under a "Privacy Guarantee" heading: the same overclaim, in the screen the user trusts | `McpServersPanel.tsx:895` | Medium |
| 3 | The module header says sender data "never reaches a cloud model", but a spoken alert says the sender's **name** through Edge TTS, a cloud service | `wa.rs` header | Low, accuracy of the claim |
| 4 | The bridge's list-tool JSON format is undocumented; the parsers are tolerant and the self-test reports what it could not read | existing | Open |

## What was built (wording only, deliberately)

* Chat card footer: "NEXUS did not mark this chat as read." (true of NEXUS's calls; no claim about the sender's screen).
* Settings panel: "NEXUS never asks the bridge to mark a chat as read during background checks. Whether the bridge itself sends read receipts has not been verified yet, so Priority People stays off until you have tested it."
* Module header: says the sender's name is spoken aloud by the cloud voice.
* `honest_wording_tests` pins all of it, and a mutation puts the old sentence back.

## Not done, and why

The only thing that matters for this feature is a **two-phone experiment** with the owner's number and a second phone: does a background read through the bridge turn the sender's ticks blue, change "online", or show "typing"? That cannot be simulated. Everything built here (allowlist, refusals, quiet start, VIP/mute) was already tested and is unchanged. The score does not rise until the experiment is done.

## Owner checklist (`docs/testing/whatsapp-read-receipt-test.md` is the full script)

1. Second phone sends a message to the owner's number. NEXUS (switch on) polls it.
2. On the second phone: are the ticks still grey? Does "last seen" or "online" change for the owner's number?
3. Record the result in `docs/testing/live-proof-ledger.md`. If ticks turn blue, leave `memcoreWhatsapp` off and tell me.
