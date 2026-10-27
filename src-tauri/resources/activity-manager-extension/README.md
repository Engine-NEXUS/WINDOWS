# Nexus Activity Manager browser extension

This Chromium extension is required for optional website tracking in Chrome,
Brave, or Edge on Windows. It sends only the hostname of the active tab in a
focused normal browser window over a localhost WebSocket. It never sends full
URLs, page titles, or history. The manifest forbids incognito/private access.

1. In Nexus Activity Manager, enable local app recording and then website
   recording. Reveal the browser connection code.
2. Open `chrome://extensions`, `brave://extensions`, or `edge://extensions`.
   Enable Developer mode, choose **Load unpacked**, and select this folder.
3. Open the extension, choose the matching browser, and paste the code.
   Repeat for each supported browser you want counted.

Website time is a subset of browser app time, so do not add the two totals.
Only public DNS hostnames are stored; IP addresses and local names are ignored.
The extension and Nexus must both be running. To stop, turn off website
recording in Nexus. Clearing activity deletes both app and website history.
