import { useState, useEffect } from "react";
import "./kit.css";

export interface LogoTileProps {
  src?: string | null;
  name: string;
  size?: number; // default 36
  rounded?: "lg" | "full"; // default "lg"
  className?: string;
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
}: LogoTileProps) {
  const [hasError, setHasError] = useState(false);

  useEffect(() => {
    setHasError(false);
  }, [src]);

  const showImage = Boolean(src && !hasError);
  const roundedClass = rounded === "full" ? "kit-logo-tile--full" : "kit-logo-tile--lg";

  if (showImage) {
    return (
      <div
        className={`kit-logo-tile kit-logo-tile--image ${roundedClass} ${className}`}
        style={{ width: size, height: size, minWidth: size, minHeight: size }}
        data-mode="image"
        title={name}
      >
        <img
          src={src!}
          alt={name}
          className="kit-logo-img"
          onError={() => setHasError(true)}
          loading="lazy"
        />
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
