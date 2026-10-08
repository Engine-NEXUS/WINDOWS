/**
 * Command Hub theme (opaque light/dark only — transparency is out per
 * user directive). Single owner for reading + applying so every view
 * agrees. Persists to localStorage for instant paint; settings.json
 * (themeMode) is the durable copy written by the settings save flow.
 */

export type ThemeMode = "light" | "dark";

export const THEME_STORAGE_KEY = "nexus-theme-mode";
export const THEME_EVENT = "nexus:theme-change";

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

/** Apply theme to the document + persist + notify other mounted views. */
export function applyThemeMode(mode: ThemeMode): void {
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
}

/**
 * Keep THIS window's theme in step with the others. Every Command Hub /
 * stage / HUD window shares the same origin, so a theme change in one window
 * fires a `storage` event in all the others; apply the stored mode on load
 * and follow later changes. Safe to call more than once.
 */
let themeSyncStarted = false;
export function initThemeSync(): void {
  if (typeof window === "undefined" || themeSyncStarted) return;
  themeSyncStarted = true;
  applyThemeMode(getThemeMode());
  window.addEventListener("storage", (e) => {
    if (e.key === THEME_STORAGE_KEY) applyThemeMode(resolveThemeMode(e.newValue));
  });
}
