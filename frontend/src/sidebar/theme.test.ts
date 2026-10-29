import { describe, expect, it, vi } from "vitest";
import { effectiveTheme, normalizePreference, resolveThemeMode } from "./theme";

describe("theme preference", () => {
  it("keeps light, dark and system; anything else is dark", () => {
    expect(normalizePreference("light")).toBe("light");
    expect(normalizePreference("dark")).toBe("dark");
    expect(normalizePreference("system")).toBe("system");
    for (const junk of [undefined, null, "", "Light", "auto", 3, {}]) expect(normalizePreference(junk)).toBe("dark");
  });

  it("an explicit choice ignores Windows; system follows it", () => {
    expect(effectiveTheme("light", true)).toBe("light");
    expect(effectiveTheme("dark", false)).toBe("dark");
    expect(effectiveTheme("system", true)).toBe("dark");
    expect(effectiveTheme("system", false)).toBe("light");
  });

  it("a saved light or dark resolves to itself without asking the OS", () => {
    expect(resolveThemeMode("light")).toBe("light");
    expect(resolveThemeMode("dark")).toBe("dark");
    expect(resolveThemeMode("nonsense")).toBe("dark");
  });
});

describe("system preference follows Windows live", () => {
  it("re-applies when the OS setting changes, and only while the choice is 'system'", async () => {
    const store = new Map<string, string>();
    let osDark = true;
    let onChange: (() => void) | null = null;
    const root = { attrs: {} as Record<string, string>, style: { colorScheme: "" }, setAttribute(k: string, v: string) { this.attrs[k] = v; } };
    vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) });
    vi.stubGlobal("document", { documentElement: root });
    vi.stubGlobal("CustomEvent", class { detail: unknown; constructor(public type: string, init?: { detail?: unknown }) { this.detail = init?.detail; } });
    vi.stubGlobal("window", {
      matchMedia: () => ({ get matches() { return osDark; }, addEventListener: (_: string, cb: () => void) => { onChange = cb; } }),
      dispatchEvent: () => true,
      addEventListener: () => {},
    });
    const fire = () => (onChange as unknown as (() => void) | null)?.();
    vi.resetModules();
    const theme = await import("./theme");

    store.set(theme.THEME_STORAGE_KEY, "system");
    theme.initThemeSync();
    expect(root.attrs["data-theme"]).toBe("dark");
    expect(root.style.colorScheme).toBe("dark");

    osDark = false;
    fire();
    expect(root.attrs["data-theme"]).toBe("light");
    expect(root.style.colorScheme).toBe("light");

    // An explicit choice ignores later OS changes.
    theme.applyThemePreference("dark", false);
    osDark = false;
    fire();
    expect(root.attrs["data-theme"]).toBe("dark");
    vi.unstubAllGlobals();
  });
});
