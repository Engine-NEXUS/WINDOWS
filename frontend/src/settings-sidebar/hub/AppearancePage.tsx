import { useState, useEffect, useRef } from "react";
import { performThemeTransition, type TransitionVariant } from "../../theme/animated-theme-toggler";
import { effectiveTheme, systemPrefersDark, type ThemePreference } from "../../sidebar/theme";

interface AppearancePageProps {
  /** The saved choice, including "system" (follow Windows). */
  themeMode: ThemePreference;
  onTheme: (m: ThemePreference) => void;
}

type ThemeChoice = ThemePreference;

export interface ColorPreset {
  id: string;
  name: string;
  hex: string;
}

export const WINDOW_BORDER_KEY = "nexus-window-border-color";
export const WINDOW_BORDER_THICKNESS_KEY = "nexus-window-border-thickness";
export const WINDOW_BORDER_OPACITY_KEY = "nexus-window-border-opacity";
export const TRANSITION_VARIANT_KEY = "nexus-transition-variant";

export const BORDER_COLORS: ColorPreset[] = [
  { id: "default", name: "Default", hex: "transparent" },
  { id: "blue", name: "Blue", hex: "#007aff" },
  { id: "indigo", name: "Indigo", hex: "#5856d6" },
  { id: "purple", name: "Purple", hex: "#af52de" },
  { id: "pink", name: "Pink", hex: "#ff2d55" },
  { id: "red", name: "Red", hex: "#ff3b30" },
  { id: "orange", name: "Orange", hex: "#ff9500" },
  { id: "yellow", name: "Yellow", hex: "#ffcc00" },
  { id: "green", name: "Green", hex: "#34c759" },
];

export const ANIMATION_VARIANTS: { id: TransitionVariant; name: string; icon: string }[] = [
  { id: "circle", name: "Circular", icon: "◯" },
  { id: "triangle", name: "Triangle", icon: "△" },
  { id: "hexagon", name: "Hexagonal", icon: "⬡" },
  { id: "diamond", name: "Diamond", icon: "◇" },
  { id: "square", name: "Square", icon: "▢" },
  { id: "star", name: "Star", icon: "★" },
];

function hexToRgba(hex: string, alphaPct: number): string {
  if (!hex || hex === "transparent" || hex === "default") return "transparent";
  let c = hex.replace("#", "");
  if (c.length === 3) c = c.split("").map((x) => x + x).join("");
  const num = parseInt(c, 16);
  if (isNaN(num)) return hex;
  const r = (num >> 16) & 255;
  const g = (num >> 8) & 255;
  const b = num & 255;
  const a = Math.max(0, Math.min(1, alphaPct / 100));
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

export function applyWindowBorderStyles(hex: string, thicknessScore: number, opacity: number) {
  try {
    localStorage.setItem(WINDOW_BORDER_KEY, hex);
    localStorage.setItem(WINDOW_BORDER_THICKNESS_KEY, String(thicknessScore));
    localStorage.setItem(WINDOW_BORDER_OPACITY_KEY, String(opacity));
  } catch {
    /* ignore */
  }

  const root = document.documentElement;
  // Map 1-100 scale to 1px-12px for prominent, responsive border thickness
  const pxWidth = Math.max(1, 1 + ((Math.max(1, Math.min(100, thicknessScore)) - 1) * 11) / 99);
  root.style.setProperty("--nx-window-border-width", `${pxWidth.toFixed(1)}px`);

  if (!hex || hex === "transparent" || hex === "default") {
    root.style.removeProperty("--nx-window-border-color");
    root.style.removeProperty("--nx-window-border-glow");
  } else {
    const effectiveColor = hexToRgba(hex, opacity);
    const effectiveGlow = hexToRgba(hex, Math.min(60, opacity * 0.45));
    root.style.setProperty("--nx-window-border-color", effectiveColor);
    root.style.setProperty("--nx-window-border-glow", effectiveGlow);
  }
}

export function initWindowBorderColor() {
  try {
    const savedColor = localStorage.getItem(WINDOW_BORDER_KEY) || "default";
    const rawThick = localStorage.getItem(WINDOW_BORDER_THICKNESS_KEY);
    const parsedThick = rawThick ? parseFloat(rawThick) : 25;
    const savedThickness = parsedThick <= 8 ? Math.round(parsedThick * 12.5) : parsedThick;
    const savedOpacity = parseFloat(localStorage.getItem(WINDOW_BORDER_OPACITY_KEY) || "100");
    applyWindowBorderStyles(savedColor, savedThickness, savedOpacity);
  } catch {
    /* ignore */
  }
}

export function AppearancePage({ themeMode, onTheme }: AppearancePageProps) {
  const [selectedTheme, setSelectedTheme] = useState<ThemeChoice>(() => themeMode);
  const [variant, setVariant] = useState<TransitionVariant>(() => {
    try {
      return (localStorage.getItem(TRANSITION_VARIANT_KEY) as TransitionVariant) || "circle";
    } catch {
      return "circle";
    }
  });

  const [selectedColor, setSelectedColor] = useState<string>(() => {
    try {
      return localStorage.getItem(WINDOW_BORDER_KEY) || "default";
    } catch {
      return "default";
    }
  });

  const [thickness, setThickness] = useState<number>(() => {
    try {
      const saved = localStorage.getItem(WINDOW_BORDER_THICKNESS_KEY);
      if (!saved) return 25;
      const parsed = parseFloat(saved);
      return parsed <= 8 ? Math.round(parsed * 12.5) : Math.round(parsed);
    } catch {
      return 25;
    }
  });

  const [opacity, setOpacity] = useState<number>(() => {
    try {
      const saved = localStorage.getItem(WINDOW_BORDER_OPACITY_KEY);
      return saved ? Math.round(parseFloat(saved)) : 100;
    } catch {
      return 100;
    }
  });

  const colorInputRef = useRef<HTMLInputElement>(null);

  // Sync state if the saved choice changes elsewhere (another window, settings reload).
  useEffect(() => {
    setSelectedTheme(themeMode);
  }, [themeMode]);

  // Apply window border styles on mount & whenever color/thickness/opacity changes
  useEffect(() => {
    applyWindowBorderStyles(selectedColor, thickness, opacity);
  }, [selectedColor, thickness, opacity]);

  const handleSelectTheme = (choice: ThemeChoice, e: React.MouseEvent<HTMLDivElement>) => {
    setSelectedTheme(choice);
    // The animation needs a concrete destination; the saved choice stays "system" so it keeps following Windows.
    const targetMode = effectiveTheme(choice, systemPrefersDark());

    if (typeof (document as any).startViewTransition === "function") {
      performThemeTransition({
        targetTheme: targetMode,
        variant,
        duration: 450,
        origin: e,
        onThemeChange: () => onTheme(choice),
      });
    } else {
      onTheme(choice);
    }
  };

  const handleSelectVariant = (v: TransitionVariant) => {
    setVariant(v);
    try {
      localStorage.setItem(TRANSITION_VARIANT_KEY, v);
    } catch {
      /* ignore */
    }
  };

  const handleSelectColor = (hex: string) => {
    setSelectedColor(hex);
    applyWindowBorderStyles(hex, thickness, opacity);
  };

  const activePreset = BORDER_COLORS.find(
    (c) =>
      c.hex.toLowerCase() === selectedColor.toLowerCase() ||
      (c.id === "default" && (selectedColor === "default" || selectedColor === "transparent"))
  );

  const activeName = activePreset
    ? activePreset.name
    : selectedColor.startsWith("#")
    ? selectedColor.toUpperCase()
    : "Custom";

  return (
    <div className="hx-page hx-appearance-page">
      {/* ── Section 1: Application Theme ── */}
      <div className="hx-appearance-section">
        <div className="hx-section-header">
          <div className="hx-section-title">Application Theme</div>
        </div>

        <div className="hx-theme-mockup-grid">
          {/* Light Theme Mockup Card */}
          <div
            className={`hx-theme-mockup-card ${selectedTheme === "light" ? "is-active" : ""}`}
            onClick={(e) => handleSelectTheme("light", e)}
            role="radio"
            aria-checked={selectedTheme === "light"}
            tabIndex={0}
          >
            <div className="hx-phone-frame hx-phone-frame--light">
              <div className="hx-phone-notch" />
              <div className="hx-mock-header">
                <div className="hx-mock-line hx-mock-line--short" />
              </div>
              <div className="hx-mock-card">
                <div className="hx-mock-row">
                  <div className="hx-mock-line hx-mock-line--med" />
                  <div className="hx-mock-switch is-on" />
                </div>
                <div className="hx-mock-line hx-mock-line--long" />
              </div>
              <div className="hx-mock-card">
                <div className="hx-mock-line hx-mock-line--med" />
                <div className="hx-mock-line hx-mock-line--short" />
              </div>
            </div>
            <div className="hx-mock-footer">
              <span className="hx-mock-label">Light</span>
              <div className="hx-mock-radio">
                {selectedTheme === "light" && <div className="hx-mock-radio-dot" />}
              </div>
            </div>
          </div>

          {/* Dark Theme Mockup Card */}
          <div
            className={`hx-theme-mockup-card ${selectedTheme === "dark" ? "is-active" : ""}`}
            onClick={(e) => handleSelectTheme("dark", e)}
            role="radio"
            aria-checked={selectedTheme === "dark"}
            tabIndex={0}
          >
            <div className="hx-phone-frame hx-phone-frame--dark">
              <div className="hx-phone-notch" />
              <div className="hx-mock-header">
                <div className="hx-mock-line hx-mock-line--short" />
              </div>
              <div className="hx-mock-card">
                <div className="hx-mock-row">
                  <div className="hx-mock-line hx-mock-line--med" />
                  <div className="hx-mock-switch is-on" />
                </div>
                <div className="hx-mock-line hx-mock-line--long" />
              </div>
              <div className="hx-mock-card">
                <div className="hx-mock-line hx-mock-line--med" />
                <div className="hx-mock-line hx-mock-line--short" />
              </div>
            </div>
            <div className="hx-mock-footer">
              <span className="hx-mock-label">Dark</span>
              <div className="hx-mock-radio">
                {selectedTheme === "dark" && <div className="hx-mock-radio-dot" />}
              </div>
            </div>
          </div>

          {/* System Split Mockup Card */}
          <div
            className={`hx-theme-mockup-card ${selectedTheme === "system" ? "is-active" : ""}`}
            onClick={(e) => handleSelectTheme("system", e)}
            role="radio"
            aria-checked={selectedTheme === "system"}
            tabIndex={0}
          >
            <div className="hx-phone-frame hx-phone-frame--split">
              <div className="hx-phone-split-left">
                <div className="hx-phone-notch" />
                <div className="hx-mock-header">
                  <div className="hx-mock-line hx-mock-line--short" />
                </div>
                <div className="hx-mock-card">
                  <div className="hx-mock-line hx-mock-line--med" />
                </div>
              </div>
              <div className="hx-phone-split-right">
                <div className="hx-mock-header" style={{ opacity: 0 }}>
                  <div className="hx-mock-line hx-mock-line--short" />
                </div>
                <div className="hx-mock-card">
                  <div className="hx-mock-switch is-on" style={{ marginLeft: "auto" }} />
                </div>
              </div>
            </div>
            <div className="hx-mock-footer">
              <span className="hx-mock-label">System</span>
              <div className="hx-mock-radio">
                {selectedTheme === "system" && <div className="hx-mock-radio-dot" />}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* ── Section 2: Theme Animation (Circular, Triangle, Hexagonal...) ── */}
      <div className="hx-appearance-section">
        <div className="hx-section-header">
          <div className="hx-section-title">Theme Animation</div>
        </div>

        <div className="hx-anim-grid">
          {ANIMATION_VARIANTS.map((a) => {
            const isSelected = variant === a.id;
            return (
              <button
                key={a.id}
                type="button"
                className={`hx-anim-card ${isSelected ? "is-selected" : ""}`}
                onClick={() => handleSelectVariant(a.id)}
              >
                <span className="hx-anim-icon">{a.icon}</span>
                <span className="hx-anim-name">{a.name}</span>
                {isSelected && <span className="hx-anim-check">✓</span>}
              </button>
            );
          })}
        </div>
      </div>

      {/* ── Section 3: Window Border (Colors + Thickness & Opacity Sliders) ── */}
      <div className="hx-appearance-section">
        <div className="hx-section-header">
          <div className="hx-section-title">Window Border</div>
        </div>

        {/* Circular Color Swatch Row (Image 2) */}
        <div className="hx-swatch-panel">
          <div className="hx-swatch-row">
            {BORDER_COLORS.map((c) => {
              const isSelected =
                (c.id === "default" && (selectedColor === "default" || selectedColor === "transparent")) ||
                c.hex.toLowerCase() === selectedColor.toLowerCase();

              return (
                <div key={c.id} className="hx-swatch-col">
                  <button
                    type="button"
                    className={`hx-swatch-btn ${isSelected ? "is-selected" : ""} ${c.id === "default" ? "is-default" : ""}`}
                    onClick={() => handleSelectColor(c.hex)}
                    title={c.name}
                    aria-label={c.name}
                  >
                    {c.id === "default" ? (
                      <span className="hx-swatch-default-icon">⊘</span>
                    ) : (
                      <span className="hx-swatch-circle" style={{ background: c.hex }} />
                    )}
                  </button>
                  {isSelected && <span className="hx-swatch-active-label">{c.name}</span>}
                </div>
              );
            })}

            {/* Custom Color Swatch Circle */}
            <div className="hx-swatch-col">
              <button
                type="button"
                className={`hx-swatch-btn hx-swatch-custom ${
                  selectedColor.startsWith("#") && !BORDER_COLORS.some((c) => c.hex.toLowerCase() === selectedColor.toLowerCase())
                    ? "is-selected"
                    : ""
                }`}
                onClick={() => colorInputRef.current?.click()}
                title="Custom color"
                aria-label="Pick custom color"
              >
                {selectedColor.startsWith("#") && !BORDER_COLORS.some((c) => c.hex.toLowerCase() === selectedColor.toLowerCase()) ? (
                  <span className="hx-swatch-circle" style={{ background: selectedColor }} />
                ) : (
                  <span className="hx-swatch-custom-icon">+</span>
                )}
              </button>
              {selectedColor.startsWith("#") && !BORDER_COLORS.some((c) => c.hex.toLowerCase() === selectedColor.toLowerCase()) && (
                <span className="hx-swatch-active-label">{activeName}</span>
              )}
              <input
                ref={colorInputRef}
                type="color"
                value={selectedColor.startsWith("#") ? selectedColor : "#3b82f6"}
                onChange={(e) => handleSelectColor(e.target.value)}
                style={{ position: "absolute", opacity: 0, pointerEvents: "none", width: 0, height: 0 }}
              />
            </div>
          </div>
        </div>

        {/* Thickness & Opacity Sliders (1 to 100 scale, no px) */}
        <div className="hx-border-controls">
          <div className="hx-slider-row">
            <span className="hx-slider-label">Thickness</span>
            <input
              type="range"
              className="hx-range-input"
              min={1}
              max={100}
              step={1}
              value={thickness}
              onChange={(e) => {
                const val = parseInt(e.target.value, 10);
                setThickness(val);
                applyWindowBorderStyles(selectedColor, val, opacity);
              }}
              onInput={(e) => {
                const val = parseInt((e.target as HTMLInputElement).value, 10);
                setThickness(val);
                applyWindowBorderStyles(selectedColor, val, opacity);
              }}
            />
            <span className="hx-slider-val">{thickness}</span>
          </div>

          <div className="hx-slider-row">
            <span className="hx-slider-label">Opacity</span>
            <input
              type="range"
              className="hx-range-input"
              min={1}
              max={100}
              step={1}
              value={opacity}
              onChange={(e) => {
                const val = parseInt(e.target.value, 10);
                setOpacity(val);
                applyWindowBorderStyles(selectedColor, thickness, val);
              }}
              onInput={(e) => {
                const val = parseInt((e.target as HTMLInputElement).value, 10);
                setOpacity(val);
                applyWindowBorderStyles(selectedColor, thickness, val);
              }}
            />
            <span className="hx-slider-val">{opacity}</span>
          </div>
        </div>
      </div>
    </div>
  );
}
