import { readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  isKnownLogoId,
  receiptLogoId,
  RECEIPT_LOGO_ALIASES,
} from "./receiptLogo";

describe("receiptLogoId resolver", () => {
  it("asserts every id returned by the alias table has an SVG file in public/activity-logos", () => {
    const dir = resolve(__dirname, "../../public/activity-logos");
    const svgFiles = new Set(
      readdirSync(dir)
        .filter((f) => f.endsWith(".svg"))
        .map((f) => f.replace(/\.svg$/, "")),
    );

    const aliases = Object.entries(RECEIPT_LOGO_ALIASES);
    expect(aliases.length).toBeGreaterThanOrEqual(40);

    for (const [alias, targetId] of aliases) {
      expect(
        svgFiles.has(targetId),
        `Alias "${alias}" maps to "${targetId}" which lacks an SVG file in public/activity-logos`,
      ).toBe(true);
      expect(isKnownLogoId(targetId)).toBe(true);
    }
  });

  const testCases: [input: string, expected: string | null][] = [
    // 1. YouTube variants
    ["youtube.com", "youtube"],
    ["YouTube", "youtube"],
    ["https://www.youtube.com/watch?v=dQw4w9WgXcQ", "youtube"],
    ["http://youtube.com", "youtube"],
    ["m.youtube.com", "youtube"],
    ["youtu.be", "youtube"],
    ["yt", "youtube"],

    // 2. Visual Studio Code variants
    ["Visual Studio Code", "vscode"],
    ["visual studio code", "vscode"],
    ["vs code", "vscode"],
    ["VS Code", "vscode"],
    ["vscode", "vscode"],

    // 3. File Explorer variants
    ["File Explorer", "fileexplorer"],
    ["file explorer", "fileexplorer"],
    ["explorer", "fileexplorer"],
    ["explorer.exe", "fileexplorer"],
    ["windows explorer", "fileexplorer"],

    // 4. Microsoft Word & Office variants
    ["Microsoft Word", "word"],
    ["microsoft word", "word"],
    ["Word", "word"],
    ["winword.exe", "word"],
    ["Microsoft Excel", "excel"],
    ["excel", "excel"],
    ["Microsoft PowerPoint", "powerpoint"],
    ["powerpoint", "powerpoint"],
    ["Microsoft OneNote", "onenote"],
    ["Microsoft OneDrive", "onedrive"],
    ["Microsoft Outlook", "outlook"],
    ["outlook.com", "outlook"],
    ["Microsoft Teams", "teams"],
    ["teams", "teams"],
    ["Microsoft To Do", "microsofttodo"],

    // 5. Browsers
    ["Edge", "edge"],
    ["Microsoft Edge", "edge"],
    ["msedge.exe", "edge"],
    ["Google Chrome", "chrome"],
    ["chrome", "chrome"],
    ["chrome.exe", "chrome"],
    ["firefox", "firefox"],
    ["brave", "brave"],

    // 6. Social & Messaging
    ["x.com", "twitter"],
    ["twitter.com", "twitter"],
    ["Twitter", "twitter"],
    ["x", "twitter"],
    ["WhatsApp", "whatsapp"],
    ["whatsapp.com", "whatsapp"],
    ["Discord", "discord"],
    ["Slack", "slack"],
    ["Spotify", "spotify"],
    ["Netflix", "netflix"],
    ["Reddit", "reddit"],
    ["reddit.com", "reddit"],

    // 7. Google Services
    ["Google Meet", "googlemeet"],
    ["meet.google.com", "googlemeet"],
    ["Google Drive", "googledrive"],
    ["Google Sheets", "googlesheets"],
    ["Google Calendar", "googlecalendar"],
    ["Google Photos", "googlephotos"],
    ["Google Cloud", "googlecloud"],
    ["Google Colab", "googlecolab"],
    ["Gmail", "gmail"],

    // 8. System & Utilities
    ["Notepad", "notepad"],
    ["notepad.exe", "notepad"],
    ["Command Prompt", "terminal"],
    ["Windows Terminal", "terminal"],
    ["cmd", "terminal"],
    ["cmd.exe", "terminal"],
    ["PowerShell", "powershell"],
    ["Windows PowerShell", "powershell"],
    ["pwsh", "powershell"],
    ["PowerToys", "powertoys"],
    ["VLC media player", "vlc"],
    ["vlc", "vlc"],

    // 9. Developer tools & AI
    ["github.com", "github"],
    ["GitHub", "github"],
    ["gitlab.com", "gitlab"],
    ["ChatGPT", "chatgpt"],
    ["OpenAI", "openai"],
    ["Claude", "claude"],
    ["Gemini", "gemini"],
    ["Cursor", "cursor"],
    ["Windsurf", "windsurf"],
    ["Zed", "zed"],
    ["Android Studio", "androidstudio"],
    ["Postman", "postman"],
    ["Figma", "figma"],
    ["Blender", "blender"],
    ["Zoom", "zoom"],

    // 10. Negatives / Unknown apps
    ["Foo Tool", null],
    ["Unknown Utility", null],
    ["Sublime Text", null],
    ["random text 123", null],
    ["", null],
    ["   ", null],
    ["https://unknown-service-abc.xyz", null],
    ["calc", null],
    ["myapp.exe", null],
  ];

  it(`correctly resolves all ${testCases.length} table test cases (>= 40 items)`, () => {
    expect(testCases.length).toBeGreaterThanOrEqual(40);
    for (const [input, expected] of testCases) {
      const resolved = receiptLogoId(input);
      expect(
        resolved,
        `Expected receiptLogoId("${input}") to be ${expected ? `"${expected}"` : "null"}, got ${resolved ? `"${resolved}"` : "null"}`,
      ).toBe(expected);
      if (resolved !== null) {
        expect(isKnownLogoId(resolved)).toBe(true);
      }
    }
  });

  it("handles null and undefined safely", () => {
    expect(receiptLogoId(null as unknown as string)).toBeNull();
    expect(receiptLogoId(undefined as unknown as string)).toBeNull();
  });
});
