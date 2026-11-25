# Deep links and handler detection: Gmail, Google Calendar, Outlook, WhatsApp on Windows

Research caveat: web search quality was limited. Most Gmail/Calendar/WhatsApp URL forms are NOT officially documented by Google/Meta; evidence is community sources. Several official pages (WhatsApp FAQ, Microsoft Learn launch-default-apps) could not be fetched (empty/404). Everything below is marked verified (seen in a fetched source) vs inferred.

## Gmail web URLs and IDs (message id vs thread id vs URL id, multi-account)

### Takeaway
Gmail's web URL form `https://mail.google.com/mail/u/<N>/#<label>/<id>` is undocumented by Google. Legacy evidence says the hex id in the URL is the Gmail *thread* id (X-GM-THRID), which equals the API `threadId` in hex; the API `message id` is also a hex string and `#all/<messageId>` is reported to work. The most robust, id-independent method is `#search/rfc822msgid:<Message-ID header>`. For multi-account, `u/<index>` is login-order dependent; `?authuser=<email>` is widely reported to work but is undocumented and needs that account signed in.

### Cited Findings
- Standard format `https://mail.google.com/mail/u/<UserId>/#label/<Label>/<UniqueId>`; UserId is the sequential index of the signed-in account (default 0); Label can be `all`. Option 1: `https://mail.google.com/mail/u/0/#all/<messageId>` using the Gmail API message id. Option 3: `https://mail.google.com/mail/u/0/#search/rfc822msgid:<Message-Id>` (header value wrapped in angle brackets), which the author calls the most reliable. Author never states explicitly that API id == URL id, and hardcodes u/0 without addressing multi-account. — [Labnol](https://Labnol.org/internet/gmail-emails-have-permanent-web-address/6811/)
- `https://mail.google.com/mail/u/0/#inbox/13216515baefe747` is described as a URL where the id is the Gmail thread id in hex, derived from IMAP X-GM-THRID converted with lowercase hex (uppercase hex reportedly not accepted). — [Limilabs](https://www.limilabs.com/blog/?p=1942)
- A Stack Overflow answer (via search snippet) says the message id is a 64-bit unsigned integer whose hex form is used in the web UI and the Gmail API; thread id is a separate attribute (X-GM-THRID). — [search result: Limilabs/SO mirror](https://kiwix.lema.org/stackoverflow.com_en_all_2023-11/questions/1687445/find-gmail-url-ids-via-imap)
- Stack Overflow question (snippet only, page not retrievable) reports the newer Gmail UI shows encoded ids in the address bar (long strings, not hex) that do not match legacy API ids; answer suggests `data-legacy-thread-id` in page HTML. — [search result](https://backiee.wasmer.app/http_Stackoverflow_com/questions/50800330/gmail-api-does-not-support-new-thread-ids)
- Gmail API reference: Message `id` = "The immutable ID of the message"; `threadId` = "The ID of the thread the message belongs to". No web URL is documented. — [Gmail API users.messages](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages)
- Apps Script guide says the API message id is not necessarily the same as the web address id and recommends rfc822msgid search (search snippet; fetch of the page did not confirm that sentence). — [Labnol](https://Labnol.org/internet/gmail-emails-have-permanent-web-address/6811/)
- Reference.Email claims a search URL built from Message-ID returns exactly one message for any recipient (vendor claim). — [Reference.Email](https://refemail.notion.site/Reference-email-b6047a506a3b42bcb0c680e960fdc19e)
- Multi-account: `u/N` index depends on login order, so same URL can hit different accounts on different browsers/devices; `https://mail.google.com/mail/u/?authuser=<email>` reportedly selects the account directly but only if all/that account is signed in to the browser. Unverified blog claims, no Google doc. — [Zenn article](https://zenn.dev/ctxzz/articles/955a9c672ce089?locale=en)

### Inferences
- Safest click target for an app holding API ids: build `https://mail.google.com/mail/u/?authuser=<email>#search/rfc822msgid%3A<url-encoded %3CMessage-ID%3E>` (needs the Message-ID header via `format=metadata&metadataHeaders=Message-ID`; it is not in the API id). Fallbacks: `#all/<API message id>` then `#inbox/<threadId>`. Combining `authuser` with a hash fragment is inferred, not found in any source.
- `#all/<id>` is label-independent so it avoids the "message not in inbox" problem of `#inbox/`; inferred from the Labnol usage, not documented.
- Because the new UI's address bar uses encoded ids, expect the legacy hex form to still be accepted (it is widely used in extensions) but this could change; build a search fallback.
- When signed into several accounts and the requested account is not signed in, Gmail will most likely show an account chooser/login; not directly sourced.
- Opening in a specific browser/profile: no Gmail-specific source. Inferred approach: launch the browser exe with the URL (`chrome.exe --profile-directory="Profile 1" <url>`; Edge similar), or rely on `authuser=<email>` within the default profile.

### Gaps
- Official Google confirmation of any Gmail web URL format, `authuser` on mail.google.com, or whether thread id vs message id opens the thread (not found).
- Exact current behavior of legacy hex ids in the new Gmail UI (no fetchable source).
- Chrome/Edge profile selection details (no source fetched).

## Gmail as installed app/PWA, mailto, Outlook, Windows Mail

### Takeaway
No source found documenting protocol handlers for Gmail PWA. For Outlook, no official Microsoft documentation of `ms-outlook:` deep links to a specific message was found; only registry presence and default-app behavior are evidenced by community sources.

### Cited Findings
- The `ms-outlook` protocol handler is registered under `HKEY_CURRENT_USER\Software\Classes\ms-outlook`; Office Quick Repair is suggested to restore it (community guide, unofficial). — [nerdtechy](https://nerdtechy.com/?p=74650)
- `mailto:` default is set in Windows Settings > Apps > Default apps; classic Outlook and new Outlook compete; users must map `.eml`, `.msg` and MAILTO explicitly to Outlook (Classic) if desired. — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/a/9520575)
- A Microsoft moderator (2023) said new Outlook did not support opening/saving .msg and .eml files (dated). — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/a/1431033)
- A `message:` link built from Message-ID is handled by macOS mail apps, not Windows (macOS report, irrelevant to Windows). — [OmniGroup forum](https://discourse.omnigroup.com/t/open-email-links-in-outlook-and-not-in-apple-mail/70599)

### Inferences
- A Gmail account read in Outlook has no stable Gmail-id deep link; best you can do is `mailto:` (compose only) or open Outlook app. Opening a particular message in Outlook would require its own item id (Graph webLink for Microsoft accounts only), which a Gmail-API-only app lacks.
- Windows Mail has no known documented per-message deep link (not researched successfully).

### Gaps
- `ms-outlook:` / `outlook://` message URL syntax: no authoritative source found.
- Graph `webLink` format and Gmail PWA/protocol handling: not researched/found.

## Google Calendar event and Meet links

### Takeaway
Use the API's `htmlLink` as-is; it already contains the `eid`. Hand-building `eid` is reverse-engineered: base64 of `"<eventId> <calendarId>"`.

### Cited Findings
- `htmlLink`: "An absolute link to this event in the Google Calendar Web UI. Read-only." `hangoutLink`: absolute link to the Google Hangout (Meet) associated with the event; `conferenceData` holds Meet details; event `id` allowed chars a-v and 0-9, 5-1024 chars; `iCalUID` per RFC5545. — [Calendar API Events](https://developers.google.com/workspace/calendar/api/v3/reference/events)
- `eid` is base64 of `<eventId> <calendar email>`, domain may be shortened to a single letter at the end (m=gmail.com, g=group.calendar.google.com, h=holiday, i=import, v=group.v); decoder from AOSP, format reverse-engineered, not an official spec. — [GitHub gist (AOSP decoder)](https://gist.github.com/louisbl/8541880)
- Stack Overflow answers: build `https://www.google.com/calendar/event?eid=<base64("eventId calendarId") without = padding>`; another form `https://calendar.google.com/event?action=TEMPLATE&tmeid=<eid>&tmsrc=<calendarId>`. Same event can live in several calendars, so the calendar id is needed. — [SO mirror](https://kaweah.freedombox.rocks/kiwix/content/stackoverflow.com_en_all_2023-11/questions/53928044/how-do-i-construct-a-link-to-a-google-calendar-event)

### Inferences
- Add `authuser=<email>` to `htmlLink` for multi-account (inferred from Gmail behavior; not sourced).
- Meet: `hangoutLink` / `conferenceData.entryPoints[].uri` (video type) are the join URLs; `meet.google.com/<code>` opens in the browser (not sourced here).

### Gaps
- `eventedit` URL form and Meet `authuser` handling: not found.

## WhatsApp

### Takeaway
Only chat-level opening is supported: `https://wa.me/<digits>` (and `api.whatsapp.com/send?phone=`); `whatsapp://send?phone=` is widely used but I found no official source. A specific message cannot be targeted.

### Cited Findings
- `wa.me/<number>` with full international number, no `+`, zeros, brackets or dashes; `?text=` prefill (URL-encoded); without number opens contact picker; `api.whatsapp.com/send?phone=` is the older form, still works (deprecation status conflicting between guides; neither is the primary source). — [u2l.ai guide](https://u2l.ai/blog/whatsapp-click-to-chat-link), [Unipile](https://www.unipile.com/es/?p=275019)
- WhatsApp Help Center "How to use click to chat" is the official source but its body could not be fetched. — [WhatsApp FAQ](https://faq.whatsapp.com/5913398998672934)
- A link router tool lists `whatsapp://...` as a native app protocol, default handler chosen in Windows Settings. An Opera forum post describes a legacy `whatsapp` key under HKCR launching `%localappdata%\WhatsApp\WhatsApp.exe` (old installer build; unconfirmed for Store app). — [win-link-router](https://gh.r-mj.com/karmaniverous/win-link-router), [Opera forum](https://forums.opera.com/post/174716)

### Inferences
- Store/native WhatsApp Desktop most likely registers `whatsapp:` (project notes in AGENTS.md already use `whatsapp://send?phone=` successfully in `wa_open.rs`), so `whatsapp://send?phone=<digits>` is the preferred app route with `wa.me` as browser fallback. Treat as project-internal evidence, not independently verified.
- Jumping to a specific message is not possible via any found scheme (absence of evidence).
- Chat opening needs a phone number; groups/contacts without number need a different route (not researched).

### Gaps
- Official WhatsApp doc text; `whatsapp:` registration for the Store package; AUMID for activation.

## Detecting installed/default handlers on Windows

### Takeaway
Default browser: read `HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice\ProgId`, then resolve `HKCR\<ProgId>\shell\open\command`. Do not write UserChoice (hash-protected).

### Cited Findings
- UserChoice ProgId path above; ProgId is a lookup key, resolved via `HKCR\<ProgId>\shell\open\command`; packaged browsers have `AppX*` ProgIds needing extra handling; `FileExts\.html\UserChoice` gave wrong results; `StartMenuInternet` is a separate key. — [search results incl. Lansweeper/SO](https://lansweeper.com/?p=18675)
- Firefox source: UserChoice has a hash; writing an incorrect hash makes Windows prompt/reset default to Edge. — [Firefox WindowsUserChoice.h](https://searchfox.org/firefox-main/source/browser/components/shell/WindowsUserChoice.h)

### Inferences
- Protocol presence for `mailto`, `whatsapp`, `ms-outlook`: check `HKCR\<scheme>` / `HKCU\Software\Classes\<scheme>` and `HKCU\...\UrlAssociations\<scheme>\UserChoice`. Prefer `ShellExecute`/default launch of the URL over explicit exe launch so Windows honors the user's choice; use explicit exe only for browser-profile targeting. AUMID activation via `shell:AppsFolder\<PFN>!App` and `Get-AppxPackage` were not verified in any fetched source (Microsoft Learn page returned 404).

### Gaps
- Microsoft Learn docs for Launcher/QueryUriSupport, AUMID activation, Get-AppxPackage: not retrieved.
