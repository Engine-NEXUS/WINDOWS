const browser = document.getElementById("browser");
const token = document.getElementById("token");
const status = document.getElementById("status");
chrome.storage.local.get(["browser", "token"]).then((saved) => {
  browser.value = saved.browser ?? "chrome";
  token.value = saved.token ?? "";
});
document.getElementById("save").addEventListener("click", async () => {
  const code = token.value.trim();
  if (!/^[a-f0-9]{64}$/i.test(code)) { status.textContent = "Enter the 64-character code shown by Nexus."; return; }
  await chrome.storage.local.set({ browser: browser.value, token: code });
  status.textContent = "Saved. Website time appears while Nexus is running.";
});
