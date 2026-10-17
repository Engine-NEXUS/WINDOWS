import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";

export type ThemeMode = "light" | "dark";

export const THEME_STORAGE_KEY = "nexus-theme-mode";
export const THEME_EVENT = "nexus:theme-change";

function isTauri(): boolean {
  return typeof window !== "undefined" && typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

/** Normalize any stored/legacy value — unknown always falls back to dark. */
export function resolveThemeMode(raw: unknown): ThemeMode {
  return raw === "light" ? "light" : "dark";
}

/** Read the effective theme (explicit stored value, else dark default). */
export function getThemeMode(): ThemeMode {
  try {
    return resolveThemeMode(localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return "dark";
  }
}

/** Apply theme to the document + persist + notify other mounted views and windows. */
export function applyThemeMode(mode: ThemeMode, broadcast = true): void {
  const next = resolveThemeMode(mode);
  try {
    localStorage.setItem(THEME_STORAGE_KEY, next);
  } catch {
    /* private mode — attribute still applies for this session */
  }
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", next);
  }
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(THEME_EVENT, { detail: next }));
  }
  if (broadcast && isTauri()) {
    emit(THEME_EVENT, next).catch(() => {});
  }
}

/**
 * Keep THIS window's theme in step with the others.
 * Syncs with settings.json, localStorage storage events, and cross-window Tauri emits.
 */
let themeSyncStarted = false;
export function initThemeSync(): void {
  if (typeof window === "undefined" || themeSyncStarted) return;
  themeSyncStarted = true;

  // Apply immediately from local cache
  applyThemeMode(getThemeMode(), false);

  // Sync with settings.json on disk if in Tauri
  if (isTauri()) {
    invoke<{ themeMode?: string }>("get_settings")
      .then((s) => {
        if (s?.themeMode) {
          applyThemeMode(resolveThemeMode(s.themeMode), false);
        }
      })
      .catch(() => {});

    // Listen for live settings updates from Rust
    listen<{ themeMode?: string }>("settings:updated", (event) => {
      if (event.payload?.themeMode) {
        applyThemeMode(resolveThemeMode(event.payload.themeMode), false);
      }
    }).catch(() => {});

    // Listen for cross-window theme broadcast
    listen<string>(THEME_EVENT, (event) => {
      if (event.payload) {
        applyThemeMode(resolveThemeMode(event.payload), false);
      }
    }).catch(() => {});
  }

  // Cross-view storage listener within same origin
  window.addEventListener("storage", (e) => {
    if (e.key === THEME_STORAGE_KEY) {
      applyThemeMode(resolveThemeMode(e.newValue), false);
    }
  });
}

