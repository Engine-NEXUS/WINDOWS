# NEXUS 5-second source-styled desktop toast: design, grouping, privacy, legal

Not legal advice. Only claims with a URL were verified this session (fetched, or taken from search snippets). Several pages were read through a summarising fetch tool, so statements are paraphrases of the page. Items I could not source are in Gaps or marked as inference.

## 1. How source apps structure notification surfaces (information hierarchy, minimum recognisable set)

### Takeaway
I could not fetch primary design specs for Gmail, Slack, Linear, Netlify, Render or Vercel's own notification surfaces. What is documented is the OS-level anatomy every desktop and mobile toast sits inside: app identity, a title, a one-line body, an optional sender image, and action buttons. The source-specific hierarchy below is a proposal built from that anatomy plus API field names.

### Cited Findings
- Windows app notifications support app-logo-override images (Microsoft's example is the sender's profile image on chat or email), inline and hero images, buttons, quick-reply, context-menu buttons and progress bars. — [Microsoft Learn: notification UX guidance](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)
- Android's minimum notification is a small icon (the only required visible element), a title and body text. Body text is truncated to one line unless the notification is expandable. Up to 3 action buttons are allowed, and they must not duplicate the tap action. — [Android: build a notification](https://developer.android.com/develop/ui/views/notifications/build-notification)
- Android MessagingStyle uses a Person (name and avatar) plus a conversation title, so group chats are not mistaken for one-to-one chats. — [Android: build a notification](https://developer.android.com/develop/ui/views/notifications/build-notification)
- GitHub Actions annotations carry severity (error, warning or notice), an optional title, file, line, endLine, col and endColumn, and a message. This is the data behind "failing step / file:line" lines in a CI summary. — [GitHub Docs: workflow commands](https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions)
- GitHub job summaries are Markdown written to `GITHUB_STEP_SUMMARY` and shown on the run summary page. This is separate from annotations. — [GitHub Docs: workflow commands](https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions)
- The Vercel dashboard shows failed or canceled build feedback on the deployment details page. — [Vercel changelog](https://vercel.com/changelog/improved-error-messages-for-failed-or-canceled-builds)
- A community report says the official Vercel Slack integration can leave a cancelled build stuck at "deploying", so Vercel's own status vocabulary there is limited. — [vercel/vercel discussion 6453](https://github.com/vercel/vercel/discussions/6453)

### Inferences
Proposed minimum recognisable set per source (own components, not copies):
- **Mail:** sender avatar (initial circle) and name, then subject (bold, 1 line), then a snippet (1 line, clipped), then a "Mail" source chip. One CTA, "Open". Optional: "Mute sender".
- **GitHub PR or CI:** a status chip (Failed, Passed, Review requested, Merged), then `repo · branch` or `#PR title`, then the failing job/step plus the first annotation (`file:line` + message), plus duration. One CTA, "View run".
- **Deploy (Vercel, Netlify, Render):** a status chip (Failed, Ready, Building), then project name and environment (Production), then branch and short commit SHA with the commit message, plus duration. For a failure, add the one-line error. One CTA, "View logs".
- **WhatsApp or chat:** contact name, a sender initial, a message-count badge. Show "New message" with no text by default.
- Recognisability comes mostly from the status colour, the source chip with its icon, and the data fields (`repo@branch`, SHA, `file:line`). Nothing needs a copied layout or a copied logo.

### Gaps
- No primary-source description of the layout of Gmail desktop or Chrome notifications, Gmail Android heads-up, or the Gmail inbox row.
- No fetched design specs for Slack, Linear, Netlify, Render, GitHub web inbox or GitHub Mobile notification layouts. The Vercel Slack failed-build message fields were not found in official docs.

## 2. Notification UX: duration, hover pause, grouping, rate limiting, urgency, sound

### Takeaway
Platform guidance says to keep notifications informative, quiet and deduplicated, and to respect focus modes. I found no authoritative number for an "optimal" heads-up duration. A 5-second toast is shorter than the legacy Windows default, so it needs hover-to-pause and a persistent log to compensate (inference).

### Cited Findings
- Older Windows Runtime docs describe a standard toast that stays 7 seconds and a long-duration toast that stays 25 seconds. This came from a search snippet of a legacy page and may be outdated. — [Microsoft Learn (toast overview)](https://learn.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/toast-notifications-overview)
- Microsoft says too many interruptions lead users to turn the channel off. It recommends suppressing a notification straight into Notification Center without an alert, and detecting Windows 11 Focus Sessions via `FocusSessionManager` to adjust behaviour. — [Microsoft Learn: notification UX guidance](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)
- Microsoft says to clear stale notifications once the content is seen elsewhere. Example: when 10 emails are viewed in the app, remove the 10 notifications, including when they were viewed on the phone or web. A click on the notification should open the app in that notification's context. — [Microsoft Learn: notification UX guidance](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)
- Android: notification sound is limited to once per second (API 27+). Updating in place by ID with `setOnlyAlertOnce()` avoids repeated alerting. Many updates per second may be dropped. `setTimeoutAfter()` auto-removes after a set duration. — [Android: build a notification](https://developer.android.com/develop/ui/views/notifications/build-notification)
- Android states the system does not guarantee the requested alert level, and the user can always change the channel importance. — [Android: build a notification](https://developer.android.com/develop/ui/views/notifications/build-notification)
- Apple: Time Sensitive is only for events happening now or within the hour (e.g. an account security issue or a delivery). Never use it for marketing. — [Apple HIG: managing notifications](https://developer.apple.com/design/human-interface-guidelines/managing-notifications.md)
- Azure DevOps toast guidance (a different product): keep text to about one line and show only one toast at a time. — [Azure DevOps toast](https://developer.microsoft.com/en-us/azure-devops/components/toast)
- Research: Iqbal and Horvitz (CSCW 2010) is a field study of desktop alerts, which exist to give awareness while the user attends to a primary task. Horvitz and Apacible (ICMI 2003) model the expected cost of interruption. Cutrell, Czerwinski and Horvitz (Interact 2001) studied how instant messages affect ongoing tasks and memory. Gluck et al. (CHI 2007) recommend matching how much attention a notification grabs to the utility of its content. I did not extract numeric results from these papers. — [Iqbal and Horvitz 2010](https://www.erichorvitz.com/shamsi_iqbal-eric_horvitz_cscw_2010.pdf); [Horvitz and Apacible](https://erichorvitz.com/learninterrupt.htm); [Cutrell et al. 2001](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/Interact2001Messaging.pdf); [Gluck et al. CHI 2007](https://www.interruptions.net/literature/Gluck-CHI07.pdf)
- Secondary summary, unverified at the source: NN/g says products overuse intrusive interruptions for unnecessary information. — [Built In summary](https://builtin.com/articles/weird-push-notification-belitsky)

### Inferences
Proposed policy for NEXUS (design recommendations, not sourced facts):
- **Urgency tiers:** Critical (production deploy or CI failure, security alert) gets a toast and a soft earcon. Normal (PR review requested, mail from a priority sender) gets a silent toast. Low (success, newsletters) is log-only, no toast.
- **Duration:** a 5-second default. Pause the timer on hover or keyboard focus and restart it on leave. Failures could get 8 to 10 seconds, but that is my suggestion with no source behind it.
- **Grouping:** a single toast slot with a queue. Collapse repeats by entity key (`repo+branch`, `project+env`, `thread`) into "+3 more", and update the toast in place instead of stacking. Keep a persistent panel or log for anything auto-dismissed.
- **Rate limiting:** at most one sound per second (borrowing Android's limit), plus a per-source cooldown. Suppress toasts during Focus Sessions or meetings and queue them silently. Dedupe on a stable event ID and use state-change semantics: report a failure once, report recovery once. Clear the toast when the user opens the item elsewhere.
- **Sound:** off by default except Critical. Never play audio while screen sharing.

### Gaps
- No authoritative "optimal heads-up duration" figure. Chrome, Android and Gmail heads-up timings were not found.
- NN/g primary articles were not retrievable. Numeric interruption-cost results from the papers were not extracted.
- Apple HIG grouping guidance was not found in the results.

## 3. Privacy: sender/subject/snippet on shared screens, redaction, capture exclusion, Gmail policy

### Takeaway
Apple and Android both say to keep private content out of notifications others can see, and Android offers a redacted "public version". For NEXUS the safe default is sender and subject only, snippets hidden in sensitive contexts, and `WDA_EXCLUDEFROMCAPTURE` on the toast window, which Microsoft says is not a security guarantee.

### Cited Findings
- Apple: avoid sensitive, personal or confidential information in notifications, because you can't predict who can see the screen. — [Apple HIG: managing notifications](https://developer.apple.com/design/human-interface-guidelines/managing-notifications.md)
- Android: `VISIBILITY_PRIVATE` shows only the icon and title on the lock screen, and `setPublicVersion()` provides a redacted alternative (e.g. "You have 3 new text messages" with no senders or contents). The user ultimately controls lock-screen visibility. — [Android: build a notification](https://developer.android.com/develop/ui/views/notifications/build-notification)
- `SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE` shows the window only on the monitor and omits it from capture. It is supported from Windows 10 version 2004, and older versions behave like `WDA_MONITOR`. It requires DWM composition, and the window must belong to the calling process. Microsoft says explicitly that it is not a security or DRM feature and does not strictly protect content. — [Microsoft Learn: SetWindowDisplayAffinity](https://learn.microsoft.com/en-us/windows/desktop/api/winuser/nf-winuser-setwindowdisplayaffinity)
- Remote Desktop sessions disable DWM, so an excluded window may still render over RDP (third-party analysis). — [IOActive](https://www.ioactive.com/the-drm-flag-that-isnt-drm/)
- Gmail restricted scopes cover reading message bodies, metadata or headers. Approved uses include email clients and productivity enhancements (the page names CRM, mail merge and generative-AI summaries). Limited Use requires data use only for features visible and prominent in the app's UI. It prohibits selling the data, transfers to ad platforms or data brokers, and ML/AI training beyond the user's own personalised model. The app must carry an affirmative Limited Use statement. — [Google Workspace user data and developer policy](https://developers.google.com/gmail/api/policy)
- The same policy: transfers are generally prohibited, with exceptions (with consent to provide the feature, for security, for legal compliance, or a merger or sale with consent). Human access is allowed only with explicit documented consent, for aggregated or anonymised internal use, for security, or for legal reasons. Permanent copies and databases of Google user data are prohibited, and so is caching longer than the cache headers allow. Users must be able to delete data. Data must be encrypted in transit and at rest, and OAuth tokens encrypted at rest. Restricted scopes require a CASA security assessment. — [Google Workspace user data and developer policy](https://developers.google.com/gmail/api/policy)
- The Gmail policy page has no Gmail-specific display rule. The general rule is that disclosures must appear in-app during normal use, not only in a privacy policy. — [Google Workspace user data and developer policy](https://developers.google.com/gmail/api/policy)
- Applications need a privacy policy that documents their data handling and must limit use to what it discloses. — [Google API Services User Data Policy](https://developers.google.com/terms/api-services-user-data-policy)

### Inferences
Proposed redaction policy:
- **Local-only pipeline:** fetch, classify and render in-process. Do not send snippets to a cloud model (that would be a transfer to evaluate against Limited Use). Do not persist bodies or snippets. Keep only IDs and timestamps, and hold sender and subject in memory with a short expiry.
- **Sensitive-content detection (heuristics, my suggestion):** OTP and verification codes (4 to 8 digits near "code", "OTP", "verify", "one-time"), password-reset and sign-in links, bank, card or UPI keywords, and security-alert senders. On a match, show only "Security/verification message" and never the code.
- **Context modes:** Normal (sender, subject and one-line snippet). Private (sender and subject only). Hidden (source chip and count only). Switch automatically to Hidden or Private when a meeting is detected, when the screen is being shared (best effort), when a window is fullscreen, or when the user sets "hide content". Apply `WDA_EXCLUDEFROMCAPTURE` to the toast window as a mitigation, not a guarantee: it does nothing on older Windows 10 builds or over RDP, and it hides the toast from the user's own screenshots.
- **GDPR data minimisation:** I did not fetch GDPR text. As a design principle, process the minimum fields and retain them for the shortest time. This is an inference and needs a legal check.

### Gaps
- No fetched source for GDPR Article 5(1)(c) data minimisation or for OTP detection practice.
- No source on how Teams, Zoom or Slack detect screen sharing for notification suppression, or whether Windows exposes a reliable "screen is being shared" signal to apps.
- I did not verify whether Google's policy treats a transient on-screen toast as "storage" or "transfer". The text reviewed does not address it.

## 4. Legal: mimicking third-party UIs, brand marks, icon licences

### Takeaway
Logos and trademarks are the clear risk. The vendors' own guidelines allow truthful, non-prominent use of their marks to show an integration, and forbid implying endorsement, modifying the marks, or using them as your own icon. Generic layout patterns (a card with an avatar, a title and a chip) are not covered by these brand pages. I found no source addressing trade dress or copyright for UI layouts, so I make no claim there.

### Cited Findings
- GitHub: permitted uses include linking to GitHub, showing that your project integrates with GitHub using a permitted logo, and keeping the logo less prominent than your own. Prohibited: implying GitHub makes or endorses your product, using its logo as your own icon, changing color or dimensions, combining it with other words or design elements, or using marks without written permission. The Invertocat appears only in white or black, and in a few cases grey or green. — [GitHub brand: logo](https://brand.github.com/foundations/logo)
- GitHub's logo policy says the logos may appear in third-party applications "in some scenarios", and Octocats cannot be used for a company or a product you build. GITHUB, OCTOCAT and their logos are registered US trademarks. — [GitHub logo policy](https://docs.github.com/en/site-policy/other-site-policies/github-logo-policy); [GitHub logos](https://github.com/logos)
- Vercel: you may use the marks to truthfully describe Vercel's products or to say you are a customer, with Vercel's trademark attribution statement. You may not use them in product or domain names, imply sponsorship or affiliation, show them more prominently than your own marks, modify them, or use them commercially in marketing. Contact brand@vercel.com for other uses. — [Vercel brand guidelines](https://vercel.com/geist/brands)
- WhatsApp (Meta): use only the official assets from the brand site. Don't alter or recolor them or combine them with other marks, don't imply endorsement, and don't use confusingly similar images. The page describes no integration allowance and says to confirm anything else with WhatsApp. — [Meta: WhatsApp brand](https://www.meta.com/brand/resources/whatsapp/whatsapp-brand/)
- Google: the Google logo may be used only with a partnership and formal approval. Don't mimic, copy, modify or incorporate Google icons or logos into your own. I found no current Gmail-specific logo page. — [Google Brand Resource Center](https://about.google/brand-resource-center/brand-elements)
- Google Workspace Marketplace: don't use Google brand elements, or modified versions, as your app's logo. — [Workspace Marketplace branding](https://developers-dot-devsite-v2-prod.appspot.com/workspace/marketplace/terms/branding?hl=it)
- Primer Octicons is under the MIT licence, copyright GitHub Inc., and the LICENSE text has no trademark exclusion. The set includes `mark-github` and `logo-github`, which remain GitHub trademarks despite the MIT icon licence (my reading, not a stated rule). — [Octicons LICENSE](https://raw.githubusercontent.com/primer/octicons/main/LICENSE); [Primer icons index](https://primer.style/foundations/icons)
- Simple Icons is CC0-1.0, but individual icons may carry their own licences or brand guidelines, and the project disclaims responsibility for brand claims. Users should read each brand's guidelines and obtain permission where needed. — [simple-icons repo](https://github.com/simple-icons/simple-icons); [DISCLAIMER.md](https://raw.githubusercontent.com/simple-icons/simple-icons/develop/DISCLAIMER.md)

### Inferences
Safe recipe ("inspired by", not imitation):
1. Build the toast from your own components and a single NEXUS visual language (own typography, spacing, radius). Reproduce information structure (sender, subject, status chip, repo, branch, SHA, failing step), not pixel layouts, color schemes or distinctive proprietary glyphs.
2. Identify the source with a small neutral badge using your own icon (envelope, branch, rocket) plus a text label ("Gmail", "GitHub", "Vercel"). Using a product name as plain text to say where the data came from is descriptive. Adding an official logo is optional and is where the vendor rules apply.
3. If you do show marks: use only the official downloaded asset, unmodified, in an allowed color, small and clearly secondary to NEXUS branding. Do not use them as the toast or app icon. Add "Not affiliated with or endorsed by..." in settings or About, and a trademark attribution line where the vendor requires one (Vercel).
4. For WhatsApp and Google (Gmail), whose pages describe no integration allowance, default to text-only labels and your own generic icons. Treat any logo use as needing permission.
5. Status colors (red for failed, green for passed) are generic and fine. Don't recreate the GitHub or Vercel branded color treatments or the Gmail red-M envelope.
6. Never name a feature or domain after a vendor mark.

### Gaps
- Brand/trademark guidelines for Netlify, Render and Supabase were not retrieved, so their rules are unknown. Slack, Linear and Material Symbols licensing (I believe Apache-2.0, unsourced) were also not checked.
- No fetched legal source on trade dress or copyright of UI layouts, and no case law. "Nominative fair use" was not researched; only the vendors' own guideline pages were read.
- Secondary sources disagree on exactly what the Vercel mark is. Rely on the official page above.
