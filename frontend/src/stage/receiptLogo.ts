// Pure resolver for app and website logos shown on the ActionReceipt card.
// Maps a subject (app display name, executable name, or URL/host) to a known activity-logos SVG id.

import { KNOWN_ICONS } from "../notify-toast/ProviderLogo";
import activityIconColors from "../settings-sidebar/hub/activityIconColors.json";

/**
 * Complete set of brand SVGs present in frontend/public/activity-logos/<id>.svg.
 */
export const KNOWN_ACTIVITY_LOGOS = new Set<string>([
  "aider",
  "androidstudio",
  "antigravity",
  "anydesk",
  "balena",
  "blackmagic",
  "blender",
  "brave",
  "canva",
  "chatgpt",
  "chrome",
  "claude",
  "clipchamp",
  "cmake",
  "codex",
  "copilot",
  "cursor",
  "davinci",
  "deepnote",
  "discord",
  "dribbble",
  "edge",
  "excel",
  "fileexplorer",
  "figma",
  "firebase",
  "firefox",
  "gemini",
  "genericapp",
  "github",
  "gitlab",
  "gmail",
  "google",
  "googlecalendar",
  "googlecloud",
  "googlecolab",
  "googledrive",
  "googlemeet",
  "googlephotos",
  "googlesheets",
  "instagram",
  "jules",
  "linkedin",
  "lottiefiles",
  "lovable",
  "microsofttodo",
  "mongodb",
  "netflix",
  "netlify",
  "nodejs",
  "notepad",
  "notion",
  "onedrive",
  "onenote",
  "openai",
  "opencode",
  "outlook",
  "perplexity",
  "postman",
  "powerpoint",
  "powershell",
  "powertoys",
  "qbittorrent",
  "rainmeter",
  "reddit",
  "render",
  "rockstar",
  "samsung",
  "signal",
  "skype",
  "slack",
  "spotify",
  "steam",
  "supabase",
  "tailscale",
  "teams",
  "telegram",
  "terminal",
  "tiktok",
  "trae",
  "twitter",
  "vercel",
  "visualstudio",
  "vlc",
  "vscode",
  "whatsapp",
  "windsurf",
  "word",
  "xbox",
  "youtube",
  "zed",
  "zoom",
]);

/**
 * Returns true if the id exists in KNOWN_ICONS, activityIconColors.json, or KNOWN_ACTIVITY_LOGOS.
 */
export function isKnownLogoId(id: string): boolean {
  if (!id) return false;
  return (
    KNOWN_ACTIVITY_LOGOS.has(id) ||
    KNOWN_ICONS.has(id) ||
    id in (activityIconColors as Record<string, string>)
  );
}

/**
 * Alias table mapping subjects, domain variants, and process names to logo ids.
 * Every target id MUST exist in KNOWN_ACTIVITY_LOGOS.
 */
export const RECEIPT_LOGO_ALIASES: Record<string, string> = {
  // Editors & IDEs
  "visual studio code": "vscode",
  "vs code": "vscode",
  "vscode": "vscode",
  "visualstudiocode": "vscode",
  "visual studio": "visualstudio",
  "devenv": "visualstudio",
  "devenv.exe": "visualstudio",
  "android studio": "androidstudio",
  "studio64.exe": "androidstudio",
  "windsurf": "windsurf",
  "cursor": "cursor",
  "zed": "zed",

  // Windows system & utilities
  "file explorer": "fileexplorer",
  "explorer": "fileexplorer",
  "windows explorer": "fileexplorer",
  "explorer.exe": "fileexplorer",
  "command prompt": "terminal",
  "windows terminal": "terminal",
  "terminal": "terminal",
  "cmd": "terminal",
  "cmd.exe": "terminal",
  "wt": "terminal",
  "wt.exe": "terminal",
  "powershell": "powershell",
  "windows powershell": "powershell",
  "powershell.exe": "powershell",
  "pwsh": "powershell",
  "pwsh.exe": "powershell",
  "powertoys": "powertoys",
  "notepad": "notepad",
  "notepad.exe": "notepad",

  // Microsoft Office & Cloud
  "microsoft word": "word",
  "ms word": "word",
  "word": "word",
  "winword": "word",
  "winword.exe": "word",
  "microsoft excel": "excel",
  "ms excel": "excel",
  "excel": "excel",
  "excel.exe": "excel",
  "microsoft powerpoint": "powerpoint",
  "ms powerpoint": "powerpoint",
  "powerpoint": "powerpoint",
  "powerpnt": "powerpoint",
  "powerpnt.exe": "powerpoint",
  "ppt": "powerpoint",
  "microsoft onenote": "onenote",
  "onenote": "onenote",
  "onenote.exe": "onenote",
  "microsoft onedrive": "onedrive",
  "onedrive": "onedrive",
  "onedrive.exe": "onedrive",
  "microsoft outlook": "outlook",
  "outlook": "outlook",
  "outlook.com": "outlook",
  "outlook.exe": "outlook",
  "microsoft teams": "teams",
  "teams": "teams",
  "teams.exe": "teams",
  "microsoft todo": "microsofttodo",
  "microsoft to do": "microsofttodo",
  "to do": "microsofttodo",
  "todo": "microsofttodo",

  // Browsers
  "microsoft edge": "edge",
  "ms edge": "edge",
  "edge": "edge",
  "msedge": "edge",
  "msedge.exe": "edge",
  "google chrome": "chrome",
  "chrome": "chrome",
  "chrome.exe": "chrome",
  "firefox": "firefox",
  "firefox.exe": "firefox",
  "brave": "brave",
  "brave.exe": "brave",

  // Social & Web
  "youtube": "youtube",
  "youtube.com": "youtube",
  "m.youtube.com": "youtube",
  "youtu.be": "youtube",
  "yt": "youtube",
  "x.com": "twitter",
  "twitter.com": "twitter",
  "twitter": "twitter",
  "x": "twitter",
  "whatsapp": "whatsapp",
  "whatsapp.com": "whatsapp",
  "web.whatsapp.com": "whatsapp",
  "reddit": "reddit",
  "reddit.com": "reddit",
  "tiktok": "tiktok",
  "tiktok.com": "tiktok",
  "linkedin": "linkedin",
  "linkedin.com": "linkedin",
  "instagram": "instagram",
  "instagram.com": "instagram",
  "telegram": "telegram",
  "telegram.org": "telegram",
  "signal": "signal",
  "signal.org": "signal",
  "discord": "discord",
  "discord.com": "discord",
  "slack": "slack",
  "slack.com": "slack",
  "spotify": "spotify",
  "spotify.com": "spotify",
  "netflix": "netflix",
  "netflix.com": "netflix",

  // Google services
  "google meet": "googlemeet",
  "meet": "googlemeet",
  "meet.google.com": "googlemeet",
  "google calendar": "googlecalendar",
  "calendar.google.com": "googlecalendar",
  "google drive": "googledrive",
  "drive.google.com": "googledrive",
  "google sheets": "googlesheets",
  "sheets.google.com": "googlesheets",
  "google photos": "googlephotos",
  "photos.google.com": "googlephotos",
  "google cloud": "googlecloud",
  "google colab": "googlecolab",
  "colab.research.google.com": "googlecolab",
  "google gemini": "gemini",
  "gemini": "gemini",
  "gemini.google.com": "gemini",
  "bard": "gemini",
  "gmail": "gmail",
  "mail.google.com": "gmail",

  // Developer & Media Tools
  "github": "github",
  "github.com": "github",
  "gitlab": "gitlab",
  "gitlab.com": "gitlab",
  "copilot": "copilot",
  "github copilot": "copilot",
  "chatgpt": "chatgpt",
  "chat gpt": "chatgpt",
  "chatgpt.com": "chatgpt",
  "chat.openai.com": "chatgpt",
  "open ai": "openai",
  "openai": "openai",
  "openai.com": "openai",
  "claude": "claude",
  "claude.ai": "claude",
  "anthropic": "claude",
  "postman": "postman",
  "postman.com": "postman",
  "vlc media player": "vlc",
  "vlc": "vlc",
  "vlc.exe": "vlc",
  "zoom": "zoom",
  "zoom meetings": "zoom",
  "zoom.us": "zoom",
  "davinci resolve": "davinci",
  "davinci": "davinci",
  "blackmagic design": "blackmagic",
  "blackmagic": "blackmagic",
  "figma": "figma",
  "blender": "blender",
  "steam": "steam",
  "steam.exe": "steam",
};

function checkValid(id: string | null | undefined): string | null {
  if (!id) return null;
  return isKnownLogoId(id) ? id : null;
}

/**
 * Maps an action receipt subject (e.g. "YouTube", "youtube.com", "Visual Studio Code",
 * "Notepad", "Command Prompt", "github.com") to an existing logo id in public/activity-logos/.
 * Returns null if the subject cannot be resolved to a known logo.
 */
export function receiptLogoId(subject: string): string | null {
  if (!subject || typeof subject !== "string") return null;
  const raw = subject.trim().toLowerCase();
  if (!raw) return null;

  // 1. Direct alias match
  if (raw in RECEIPT_LOGO_ALIASES) {
    return checkValid(RECEIPT_LOGO_ALIASES[raw]);
  }

  // 2. Strip protocol/scheme (e.g. https://, http://)
  let s = raw.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");

  // Strip path, query, hash (e.g. youtube.com/watch?v=123 -> youtube.com)
  s = s.split(/[/?#]/)[0];

  // Strip port (e.g. localhost:3000 -> localhost)
  s = s.replace(/:\d+$/, "");

  // Strip leading "www."
  s = s.replace(/^www\./, "");

  // Check alias with stripped host
  if (s in RECEIPT_LOGO_ALIASES) {
    return checkValid(RECEIPT_LOGO_ALIASES[s]);
  }

  // 3. Strip common TLDs (e.g. youtube.com -> youtube, github.com -> github, x.com -> x)
  const domainMatch = s.match(/^([^.]+)\.(?:[a-z]{2,3}\.[a-z]{2}|[a-z]{2,})$/);
  if (domainMatch) {
    const domainBase = domainMatch[1];
    if (domainBase in RECEIPT_LOGO_ALIASES) {
      return checkValid(RECEIPT_LOGO_ALIASES[domainBase]);
    }
    if (isKnownLogoId(domainBase)) {
      return domainBase;
    }
    const cleanDomainBase = domainBase.replace(/[^a-z0-9]/g, "");
    if (cleanDomainBase in RECEIPT_LOGO_ALIASES) {
      return checkValid(RECEIPT_LOGO_ALIASES[cleanDomainBase]);
    }
    if (isKnownLogoId(cleanDomainBase)) {
      return cleanDomainBase;
    }
  }

  // 4. Strip .exe if present (e.g. notepad.exe -> notepad)
  const withoutExe = raw.replace(/\.exe$/i, "");
  if (withoutExe !== raw) {
    if (withoutExe in RECEIPT_LOGO_ALIASES) {
      return checkValid(RECEIPT_LOGO_ALIASES[withoutExe]);
    }
    if (isKnownLogoId(withoutExe)) {
      return withoutExe;
    }
  }

  // 5. Strip all non-alphanumerics (e.g. "Google Meet" -> "googlemeet")
  const alphanumeric = raw.replace(/[^a-z0-9]/g, "");
  if (alphanumeric in RECEIPT_LOGO_ALIASES) {
    return checkValid(RECEIPT_LOGO_ALIASES[alphanumeric]);
  }
  if (isKnownLogoId(alphanumeric)) {
    return alphanumeric;
  }

  return null;
}
