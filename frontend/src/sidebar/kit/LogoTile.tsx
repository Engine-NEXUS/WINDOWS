import { useState, useEffect } from "react";
import "./kit.css";
import { categoryGlyph, techIconFor } from "./techIcon";

export interface LogoTileProps {
  src?: string | null;
  name: string;
  size?: number; // default 36
  rounded?: "lg" | "full"; // default "lg"
  className?: string;
  /** Detection id (e.g. "nextjs"): resolves a bundled local logo before anything else. */
  techId?: string | null;
  /** Detection category: picks a glyph when no brand mark exists. */
  category?: string | null;
}

/**
 * Computes a deterministic hue [0, 360) from a string.
 */
export function hashHue(str: string): number {
  if (!str) return 210;
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = (hash << 5) - hash + str.charCodeAt(i);
    hash |= 0;
  }
  return Math.abs(hash) % 360;
}

export function LogoTile({
  src,
  name,
  size = 36,
  rounded = "lg",
  className = "",
  techId,
  category,
}: LogoTileProps) {
  const [hasError, setHasError] = useState(false);
  // A logo that is already one of the app's own files ("/activity-logos/..") is kept as given.
  const ownFile = Boolean(src && src.startsWith("/"));
  const local = ownFile ? null : techIconFor(techId, name);
  // a bundled mark first (instant, offline), then a supplied logo
  const [localFailed, setLocalFailed] = useState(false);
  const imgSrc = local && !localFailed ? local.src : src;
  const tone = local && !localFailed ? local.tone : "light";

  useEffect(() => {
    setHasError(false);
    setLocalFailed(false);
  }, [src, techId, name]);

  const showImage = Boolean(imgSrc && !hasError);
  const roundedClass = rounded === "full" ? "kit-logo-tile--full" : "kit-logo-tile--lg";

  if (showImage) {
    return (
      <div
        className={`kit-logo-tile kit-logo-tile--image ${roundedClass} ${className}`}
        style={{ width: size, height: size, minWidth: size, minHeight: size }}
        data-mode="image"
        data-tone={tone}
        title={name}
      >
        <img
          src={imgSrc!}
          alt={name}
          className="kit-logo-img"
          onError={() => (local && !localFailed ? setLocalFailed(true) : setHasError(true))}
          loading="lazy"
        />
      </div>
    );
  }

  const glyph = categoryGlyph(category, techId);
  if (glyph) {
    return (
      <div
        className={`kit-logo-tile kit-logo-tile--glyph ${roundedClass} ${className}`}
        style={{
          width: size,
          height: size,
          minWidth: size,
          minHeight: size,
          color: `hsl(${glyph.hue}, 72%, 62%)`,
          backgroundColor: `hsla(${glyph.hue}, 60%, 50%, 0.16)`,
        }}
        data-mode="glyph"
        title={`${name} · ${glyph.label}`}
      >
        <span className="kit-logo-glyph" style={{ width: size * 0.58, height: size * 0.58 }}>
          {glyph.svg}
        </span>
      </div>
    );
  }

  const cleanName = (name || "?").trim();
  const letter = (cleanName[0] || "?").toUpperCase();
  const hue = hashHue(cleanName);
  // 100% solid color (no alpha)
  const bgColor = `hsl(${hue}, 65%, 40%)`;
  const fontSize = Math.max(11, Math.round(size * 0.45));

  return (
    <div
      className={`kit-logo-tile kit-logo-tile--monogram ${roundedClass} ${className}`}
      style={{
        width: size,
        height: size,
        minWidth: size,
        minHeight: size,
        backgroundColor: bgColor,
        fontSize,
      }}
      data-mode="monogram"
      data-monogram={letter}
      title={name}
    >
      <span className="kit-logo-monogram">{letter}</span>
    </div>
  );
}
