import { useEffect, useState } from "react";
import { getThemeMode, THEME_EVENT, type ThemeMode } from "./theme";

/** Current Light/Dark mode of this window (follows live changes). */
export function useThemeMode(): ThemeMode {
  const [mode, setMode] = useState<ThemeMode>(getThemeMode());
  useEffect(() => {
    const onChange = (e: Event) => {
      const m = (e as CustomEvent).detail;
      setMode(m === "light" ? "light" : "dark");
    };
    window.addEventListener(THEME_EVENT, onChange);
    return () => window.removeEventListener(THEME_EVENT, onChange);
  }, []);
  return mode;
}
