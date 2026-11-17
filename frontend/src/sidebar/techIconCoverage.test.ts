import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { categoryGlyph, techIconFor, techIconForHost } from "./kit/techIcon";

const table = JSON.parse(readFileSync("../src-tauri/src/site_fingerprints.json", "utf-8")).technologies as {
  id: string;
  name: string;
  category: string;
}[];

describe("technology logo coverage", () => {
  it("every technology the website scanner can detect has a bundled logo or a glyph, never only a letter", () => {
    const bare = table.filter((t) => !techIconFor(t.id, t.name) && !categoryGlyph(t.category, t.id));
    expect(bare.map((t) => t.id)).toEqual([]);
  });

  it("every app-scan technology has a logo or a glyph", () => {
    const app = [
      ["electron", "app-framework"], ["chromium", "browser-engine"], ["nodejs", "runtime"], ["tauri-webview2", "app-framework"],
      ["webview2", "app-framework"], ["cef", "app-framework"], ["nwjs", "app-framework"], ["qt", "ui-toolkit"],
      ["wxwidgets", "ui-toolkit"], ["gtk", "ui-toolkit"], ["imgui", "ui-toolkit"], ["flutter", "app-framework"],
      ["react-native-windows", "app-framework"], ["avalonia", "ui-toolkit"], ["maui", "app-framework"], ["wpf", "ui-toolkit"],
      ["winforms", "ui-toolkit"], ["winui", "ui-toolkit"], ["dotnet", "runtime"], ["java", "runtime"], ["python", "runtime"],
      ["go", "runtime"], ["rust", "runtime"], ["delphi-cppbuilder", "runtime"], ["autohotkey", "runtime"],
      ["squirrel", "packaging"], ["electron-builder", "packaging"], ["msix", "packaging"], ["innosetup", "packaging"],
      ["nsis", "packaging"], ["clickonce", "packaging"], ["gecko", "browser-engine"], ["unity", "engine"],
      ["unreal", "engine"], ["godot", "engine"], ["gamemaker", "engine"],
    ];
    const bare = app.filter(([id, cat]) => !techIconFor(id, id) && !categoryGlyph(cat, id));
    expect(bare).toEqual([]);
  });

  it("resolves names with versions, aliases and hosts", () => {
    expect(techIconFor(null, "Next.js 15")?.src).toBe("/tech-icons/nextjs.svg");
    expect(techIconFor(null, "Visual Studio Code")?.src).toBe("/tech-icons/vscode.svg");
    expect(techIconFor("zepto", "Zepto.js")?.src).toContain("/tech-icons/scraped/zepto.");
    expect(techIconForHost("www.googletagmanager.com")).not.toBeNull();
    expect(techIconForHost("cdn.example-unknown.test")).toBeNull();
  });
});
