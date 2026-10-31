import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";

/** What a window actually renders. */
export type ThemeMode = "light" | "dark";
/** What the owner chose. "system" follows Windows live. */
export type ThemePreference = ThemeMode | "system";

export const THEME_STORAGE_KEY = "nexus-theme-mode";
export const THEME_EVENT = "nexus:theme-change";

function isTauri(): boolean {
  return typeof window !== "undefined" && typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

/** Normalise any stored or legacy value to a preference; unknown falls back to dark (the long-standing default). */
export function normalizePreference(raw: unknown): ThemePreference {
  return raw === "light" || raw === "system" ? raw : "dark";
}

/** The theme to render for a preference. Pure: `systemDark` is what Windows currently asks for. */
export function effectiveTheme(pref: ThemePreference, systemDark: boolean): ThemeMode {
  if (pref === "system") return systemDark ? "dark" : "light";
  return pref;
}

/** Is Windows (or the browser) in dark mode right now? Dark when it cannot tell. */
export function systemPrefersDark(): boolean {
  try {
    return typeof window === "undefined" || !window.matchMedia ? true : window.matchMedia("(prefers-color-scheme: dark)").matches;
  } catch {
    return true;
  }
}

/** Resolve any stored/legacy value to what should be rendered ("system" resolves against the OS now). */
export function resolveThemeMode(raw: unknown): ThemeMode {
  return effectiveTheme(normalizePreference(raw), systemPrefersDark());
}

/** The saved preference of this window's origin (explicit stored value, else dark). */
export function getThemePreference(): ThemePreference {
  try {
    return normalizePreference(localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return "dark";
  }
}

/** Read the effective theme of this window. */
export function getThemeMode(): ThemeMode {
  return effectiveTheme(getThemePreference(), systemPrefersDark());
}

/**
 * Apply a preference to the document + persist + notify other mounted views and windows.
 * Sets `data-theme` and the CSS `color-scheme` (native scrollbars, form controls and the canvas follow it).
 */
export function applyThemePreference(pref: ThemePreference, broadcast = true): void {
  const next = normalizePreference(pref);
  const mode = effectiveTheme(next, systemPrefersDark());
  try {
    localStorage.setItem(THEME_STORAGE_KEY, next);
  } catch {
    /* private mode — attribute still applies for this session */
  }
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", mode);
    document.documentElement.style.colorScheme = mode;
  }
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(THEME_EVENT, { detail: mode }));
  }
  if (broadcast && isTauri()) {
    // The preference travels, not the result: every window resolves "system" against the same OS setting.
    emit(THEME_EVENT, next).catch(() => {});
  }
}

/** Kept for the callers that only ever pass Light or Dark. */
export function applyThemeMode(mode: ThemePreference, broadcast = true): void {
  applyThemePreference(mode, broadcast);
}

/**
 * Keep THIS window's theme in step with the others.
 * Syncs with settings.json, localStorage storage events, cross-window Tauri emits, and Windows itself.
 */
let themeSyncStarted = false;
export function initThemeSync(): void {
  if (typeof window === "undefined" || themeSyncStarted) return;
  themeSyncStarted = true;

  // Apply immediately from local cache (the boot script has normally done this before first paint).
  applyThemePreference(getThemePreference(), false);

  // "System" follows Windows live: a change in the OS setting re-applies the saved preference.
  try {
    window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (getThemePreference() === "system") applyThemePreference("system", false);
    });
  } catch {
    /* no matchMedia (tests): nothing to follow */
  }

  // Sync with settings.json on disk if in Tauri
  if (isTauri()) {
    invoke<{ themeMode?: string }>("get_settings")
      .then((s) => {
        if (s?.themeMode) {
          applyThemePreference(normalizePreference(s.themeMode), false);
        }
      })
      .catch(() => {});

    // Listen for live settings updates from Rust
    listen<{ themeMode?: string }>("settings:updated", (event) => {
      if (event.payload?.themeMode) {
        applyThemePreference(normalizePreference(event.payload.themeMode), false);
      }
    }).catch(() => {});

    // Listen for cross-window theme broadcast
    listen<string>(THEME_EVENT, (event) => {
      if (event.payload) {
        applyThemePreference(normalizePreference(event.payload), false);
      }
    }).catch(() => {});
  }

  // Cross-view storage listener within same origin
  window.addEventListener("storage", (e) => {
    if (e.key === THEME_STORAGE_KEY) {
      applyThemePreference(normalizePreference(e.newValue), false);
    }
  });
}
