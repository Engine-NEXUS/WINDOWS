# Live run: reverse engineer and Maps in the isolated app (2026-11-25)

Build: contributor-profile debug build of this branch (`node scripts/build-isolated-test.mjs`), started with `scripts/run-isolated-test.ps1` on its own hidden Windows desktop in quiet mode. Turns sent through the app's own `orchestrator_process` (`scripts/drive-isolated-turns.mjs`). The owner's own NEXUS kept running and was not touched. No API keys in this profile. No speech recognition, no audible output.

The first reverse-engineer run (same day) failed every turn on the foreground-window check; the tables below are the runs after that fix. The "avoid tolls" wording and the failed-scan sidebar were fixed after these runs and have unit tests only.

## Reverse engineer

| Turn | ms | Spoken |
|---|---|---|
| reverse engineer nextjs.org | 12881 | Okay sir, give me a moment. / I found 14 technologies, sir, but I couldn't walk through the page. (guard: 5 tunnels, 0 refused) |
| reverse engineer svelte.dev | 6892 | Okay sir, give me a moment. / I found 8 technologies, sir, but I couldn't walk through the page. (guard: 6 tunnels, 0 refused) |
| what is wordpress.org built with | 11013 | Okay sir, give me a moment. / I found 6 technologies, sir, but I couldn't walk through the page. (guard: 11 tunnels, 0 refused) |
| reverse engineer woocommerce.com | 12542 | On it, sir. This will take a minute. / I found 19 technologies, sir, but I couldn't walk through the page. (guard: 74 tunnels, 0 refused) |
| reverse engineer indeed.com | 5872 | Okay sir, give me a moment. / I found 11 technologies, sir, but I couldn't walk through the page. (guard: 23 tunnels, 0 refused) |
| reverse engineer localtest.me | 205 | I only scan sites on the public internet, sir. |
| reverse engineer paypal.com/myaccount | 40 | I do not scan private or banking sites, sir. |
| reverse engineer this website | 18 | I couldn't identify the foreground window. Please try again. |
| reverse engineer doesnotexist-nexus-test.dev | 64 | The site did not answer, sir — DNS lookup failed: No such host is known. (os error 11001) |

## Maps

| Turn | ms | Spoken |
|---|---|---|
| how far is Charminar | 2549 | Charminar is 22 kilometres away, about 29 minutes by car via Hyderabad - Mancherial Highway, without live traffic, arriving around 12:02 AM, sir. Add  |
| directions to Charminar | 1633 | The route to charminar is open, sir. It's 22 kilometres, about 29 minutes by car via Hyderabad - Mancherial Highway, without live traffic, arriving ar |
| read the steps | 8 | Step 1: Head west, then 150 metres to the next turn. Step 2: At the end of the road turn right, then 420 metres to the next turn. Step 3: Turn left, t |
| next step | 6 | Step 4: Continue onto Generals Road, then 330 metres to the next turn. |
| next step | 9 | Step 5: Turn left onto Yapral-Ammuguda road, then 1 kilometre to the next turn. |
| make it walking | 682 | Switched to walking, sir. It's 22 kilometres, about 4 hours 48 minutes on foot, arriving around 4:22 AM. |
| make it cycling | 627 | Switched to cycling, sir. It's 21 kilometres, about 1 hour 25 minutes by bike, arriving around 12:59 AM. |
| make it driving | 621 | Switched to driving, sir. It's 22 kilometres, about 29 minutes by car via Hyderabad - Mancherial Highway, without live traffic, arriving around 12:03  |
| avoid tolls | 614 | Avoiding tolls, sir. It's 22 kilometres, about 29 minutes by car via Hyderabad - Mancherial Highway, without live traffic, avoiding tolls, arriving ar |
| add a stop at Gachibowli | 1622 | Added gachibowli as a stop, sir. It's 53 kilometres, about 1 hour 11 minutes by car via Old Bombay Highway, without live traffic, avoiding tolls, with |
| remove the stops | 598 | Removed the stops, sir. It's 22 kilometres, about 29 minutes by car via Hyderabad - Mancherial Highway, without live traffic, avoiding tolls, arriving |
| how long now | 586 | Charminar is 22 kilometres away, about 29 minutes by car via Hyderabad - Mancherial Highway, without live traffic, avoiding tolls, arriving around 12: |
| directions to Atlantis Narnia | 621 | Opening the route to atlantis narnia, sir. I couldn't find atlantis narnia on the open map, sir. Say its full name and the city. |
| directions to home | 16 | I don't know where your home is yet, sir. Say: remember that my home is, followed by the address. |
| directions to Apartment #4 Banjara Hills | 4091 | The route to apartment #4 banjara hills is open, sir. It's 18 kilometres, about 25 minutes by car via Hyderabad - Mancherial Highway, without live tra |
| directions to Charminar | 1707 | The route to charminar is open, sir. It's 22 kilometres, about 29 minutes by car via Hyderabad - Mancherial Highway, without live traffic, arriving ar |
| directions to Charminar | 1653 | The route to charminar is open, sir. It's 22 kilometres, about 29 minutes by car via Hyderabad - Mancherial Highway, without live traffic, arriving ar |
| make it public transport | 26 | Switched to public transport, sir. It's 22 kilometres. Public transport times need a Maps key, sir. The map shows them. |
| remind me to leave for Charminar at 11 pm | 9 | That time has already passed today, sir. |

Also run: "remind me to leave for Charminar at 11:59 pm" -> answered that leaving now was already needed (about 29 minutes away); "what is stripe.com built with" -> refused as a private or banking site.

In the second run each scan's summary ("This is built with Next.js 16.5.0 on Vercel, styled with Tailwind CSS. I found 14 technologies; 8 are confirmed.") was queued about 4 ms before the closing line shown above and was cut off by it. A third run after the fix spoke the summary alone: "This is built with Next.js 16.5.0 on Vercel, styled with Tailwind CSS. I found 14 technologies; 8 are confirmed." and the stack sidebar showed the technology list, categories and page weight.
