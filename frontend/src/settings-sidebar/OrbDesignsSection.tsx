import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DottedOrb } from "../avatar/dotted/DottedOrb";
import { ORB_DESIGNS, type OrbDesign } from "../avatar/dotted/resolveOrb";
import { DEFAULT_ORB_DESIGNS, designFor, normalizeOrbDesigns, ORB_ACTIVITIES } from "../avatar/orbActivity";

const DESIGN_LABELS: Record<OrbDesign, string> = {
  breathing: "Breathing ring",
  listening: "Listening waves",
  working: "Working orbits",
  searching: "Searching globe",
  connecting: "Connecting web",
  weaving: "Weaving strands",
  composing: "Composing sash",
  solving: "Solving bands",
  shaping: "Shaping outline",
};

export interface OrbDesignsSectionProps {
  /** Saved mapping from settings (may be missing or partly invalid). */
  value: unknown;
  /** Keep the parent's settings draft in step so a later Save does not restore the old mapping. */
  onSaved: (next: Record<string, string>) => void;
}

/**
 * Pick which orb design NEXUS shows for each activity. Applies at once (the stage hears the settings
 * broadcast) and previews each choice with the real orb.
 */
export function OrbDesignsSection({ value, onSaved }: OrbDesignsSectionProps) {
  const [saved, setSaved] = useState<Record<string, string>>(() => normalizeOrbDesigns(value));
  useEffect(() => setSaved(normalizeOrbDesigns(value)), [value]);

  const persist = (next: Record<string, string>) => {
    setSaved(next);
    onSaved(next);
    invoke("settings_patch", { patch: { orbDesigns: next } }).catch(() => {});
  };

  return (
    <div className="settings-section">
      <div className="settings-section-title">Orb designs</div>
      <div className="setting-desc" style={{ marginBottom: 10 }}>
        What the orb looks like for each thing NEXUS does. Changes apply right away.
      </div>
      {ORB_ACTIVITIES.map(({ id, label }) => {
        const design = designFor(id, saved);
        return (
          <div key={id} className="setting-row" style={{ alignItems: "center", gap: 10 }}>
            <div
              style={{
                width: 56,
                height: 56,
                borderRadius: 12,
                background: "var(--stage-card-black)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
              }}
            >
              <DottedOrb design={design} size={48} dark aria-label={label} />
            </div>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="setting-label">{label}</div>
            </div>
            <div className="setting-control">
              <select
                className="settings-input"
                value={design}
                onChange={(e) => persist({ ...saved, [id]: e.target.value })}
                aria-label={`Orb design for ${label}`}
              >
                {ORB_DESIGNS.map((d) => (
                  <option key={d} value={d}>
                    {DESIGN_LABELS[d]}
                    {DEFAULT_ORB_DESIGNS[id] === d ? " (default)" : ""}
                  </option>
                ))}
              </select>
            </div>
          </div>
        );
      })}
      <button
        type="button"
        className="settings-btn settings-btn--glass"
        style={{ width: "100%", marginTop: 8 }}
        onClick={() => persist({})}
      >
        Reset to defaults
      </button>
    </div>
  );
}
