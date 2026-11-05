import type { CSSProperties } from "react";
import { LogoTile } from "../sidebar/kit/LogoTile";
import activityIconColors from "../settings-sidebar/hub/activityIconColors.json";

const ICON_COLORS = activityIconColors as Record<string, string>;

/** Recognized provider icon ids from Rust payload */
export const KNOWN_ICONS = new Set<string>([
  "github",
  "gmail",
  "vercel",
  "render",
  "netlify",
  "supabase",
  "whatsapp",
  "googlecalendar",
  "claude",
  "codex",
  "antigravity",
  "opencode",
  "cursor",
  "gemini",
  "copilot",
  "windsurf",
  "aider",
]);

/** Returns the local asset path for a provider logo, or null for monogram fallback (empty, "agent", unknown). */
export function logoSrc(icon?: string | null): string | null {
  if (!icon) return null;
  const id = icon.trim().toLowerCase();
  if (!id || id === "agent") return null;
  if (KNOWN_ICONS.has(id) || id in ICON_COLORS) {
    return `/activity-logos/${id}.svg`;
  }
  return null;
}

/** Returns the brand tile background color from activityIconColors.json, or undefined if unknown or empty. */
export function tileColor(icon?: string | null): string | undefined {
  if (!icon) return undefined;
  const id = icon.trim().toLowerCase();
  if (!id || id === "agent") return undefined;
  // Marks drawn in black or near-black vanish on their dark brand tile, so the toast gives them a light one.
  if (id === "render") return "#ffffff";
  return ICON_COLORS[id];
}

export interface ProviderLogoProps {
  icon?: string | null;
  provider: string;
  className?: string;
}

/** Compact 28 px app logo tile wrapping LogoTile with per-icon brand background. */
export function ProviderLogo({ icon, provider, className = "" }: ProviderLogoProps) {
  const src = logoSrc(icon);
  const bg = tileColor(icon);
  const style: CSSProperties | undefined = bg
    ? ({
        "--nt-logo-bg": bg,
        "--kit-tile-bg": bg,
      } as CSSProperties)
    : undefined;

  return (
    <div className={`nt-provider-logo ${className}`.trim()} style={style}>
      <LogoTile src={src} name={provider} size={28} />
    </div>
  );
}
