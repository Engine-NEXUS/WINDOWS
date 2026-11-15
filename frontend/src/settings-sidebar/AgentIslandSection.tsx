import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { clampScale, SCALE_DEFAULT, SCALE_MAX, SCALE_MIN } from "../agent-island/agentIslandModel";

interface IslandSettings {
  agentMonitor: boolean;
  agentMonitorHover: boolean;
  agentMonitorDisplay: "stay-awake" | "blank";
  agentMonitorWelcomeAfterMin: number;
  agentAutoHook: boolean;
  agentIslandHideWatched: boolean;
  agentIslandScale: number;
}

const DEFAULTS: IslandSettings = {
  agentMonitor: true,
  agentMonitorHover: true,
  agentMonitorDisplay: "stay-awake",
  agentMonitorWelcomeAfterMin: 0,
  agentAutoHook: true,
  agentIslandHideWatched: true,
  agentIslandScale: SCALE_DEFAULT,
};

/** Command Hub card for the Agent Island: logos of running coding agents at the top of the screen, hover for
 *  tracking, Allow / Deny cards, and the screen kept awake while they work. Saves each change at once through
 *  `settings_patch` (only the changed key), so it never overwrites other settings. */
export function AgentIslandSection() {
  const [s, setS] = useState<IslandSettings>(DEFAULTS);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    invoke<Partial<IslandSettings>>("get_settings")
      .then((r) =>
        setS({
          agentMonitor: r.agentMonitor ?? DEFAULTS.agentMonitor,
          agentMonitorHover: r.agentMonitorHover ?? DEFAULTS.agentMonitorHover,
          agentMonitorDisplay: r.agentMonitorDisplay === "blank" ? "blank" : "stay-awake",
          agentMonitorWelcomeAfterMin: r.agentMonitorWelcomeAfterMin ?? DEFAULTS.agentMonitorWelcomeAfterMin,
          agentAutoHook: r.agentAutoHook ?? DEFAULTS.agentAutoHook,
          agentIslandHideWatched: r.agentIslandHideWatched ?? DEFAULTS.agentIslandHideWatched,
          agentIslandScale: clampScale(r.agentIslandScale ?? DEFAULTS.agentIslandScale),
        }),
      )
      .catch(() => {});
  }, []);

  const save = async <K extends keyof IslandSettings>(key: K, value: IslandSettings[K]) => {
    const before = s;
    setS({ ...s, [key]: value });
    setErr(null);
    try {
      await invoke("settings_patch", { patch: { [key]: value } });
    } catch (e) {
      setS(before);
      setErr(`Could not save: ${String(e)}`);
    }
  };

  return (
    <div className="settings-section">
      <div className="settings-section-title">Agent Island</div>
      <label className="setting-row">
        <div>
          <div className="setting-label">Connect my coding agents automatically</div>
          <div className="setting-desc">NEXUS looks for Claude Code, Codex, Antigravity, OpenCode, Gemini CLI and others on this laptop and connects them so it knows when they run and stop. A backup of each one's settings is made first.</div>
        </div>
        <input type="checkbox" checked={s.agentAutoHook} onChange={(e) => save("agentAutoHook", e.target.checked)} />
      </label>
      <label className="setting-row">
        <div>
          <div className="setting-label">Show running agents</div>
          <div className="setting-desc">Logos of your coding agents at the top of the screen. Also keeps the screen awake while they work.</div>
        </div>
        <input type="checkbox" checked={s.agentMonitor} onChange={(e) => save("agentMonitor", e.target.checked)} />
      </label>
      <label className="setting-row">
        <div>
          <div className="setting-label">Only show agents I am not watching</div>
          <div className="setting-desc">An agent whose window is in front of you, or visible next to another one in a split screen, stays off the island. A permission request always shows.</div>
        </div>
        <input type="checkbox" checked={s.agentIslandHideWatched} disabled={!s.agentMonitor} onChange={(e) => save("agentIslandHideWatched", e.target.checked)} />
      </label>
      <label className="setting-row">
        <div>
          <div className="setting-label">Island size · {Math.round(s.agentIslandScale * 100)}%</div>
          <div className="setting-desc">Drag to resize the bar. It changes on your screen as you drag; "Show on my screen" displays it for ten seconds with two sample agents.</div>
        </div>
        <input
          type="range"
          min={SCALE_MIN}
          max={SCALE_MAX}
          step={0.05}
          value={s.agentIslandScale}
          disabled={!s.agentMonitor}
          onChange={(e) => save("agentIslandScale", clampScale(Number(e.target.value)))}
          aria-label="Island size"
        />
      </label>
      <div className="setting-row">
        <div>
          <div className="setting-label">Preview</div>
          <div className="setting-desc">Shows the island for ten seconds with two sample agents, at the size above.</div>
        </div>
        <button type="button" disabled={!s.agentMonitor} onClick={() => invoke("agent_island_preview").catch((e) => setErr(`Could not show the island: ${String(e)}`))}>
          Show on my screen
        </button>
      </div>
      <label className="setting-row">
        <div>
          <div className="setting-label">Open on hover</div>
          <div className="setting-desc">Hover the logos to see live tracking. A permission request always opens it.</div>
        </div>
        <input type="checkbox" checked={s.agentMonitorHover} disabled={!s.agentMonitor} onChange={(e) => save("agentMonitorHover", e.target.checked)} />
      </label>
      <label className="setting-row">
        <div>
          <div className="setting-label">When every agent is done</div>
          <div className="setting-desc">Let the screen sleep by your normal Windows settings, or also turn the display off once you have been away.</div>
        </div>
        <select value={s.agentMonitorDisplay} disabled={!s.agentMonitor} onChange={(e) => save("agentMonitorDisplay", e.target.value === "blank" ? "blank" : "stay-awake")}>
          <option value="stay-awake">Release the screen</option>
          <option value="blank">Turn the display off</option>
        </select>
      </label>
      <label className="setting-row">
        <div>
          <div className="setting-label">"While you were away" also after</div>
          <div className="setting-desc">NEXUS says "While you were away" when the PC went to sleep. Set minutes here to also say it after a long absence without sleep; 0 = only after sleep. Otherwise it simply says "Claude Code has finished, sir."</div>
        </div>
        <input
          type="number"
          min={0}
          max={1440}
          value={s.agentMonitorWelcomeAfterMin}
          onChange={(e) => save("agentMonitorWelcomeAfterMin", Math.max(0, Math.min(1440, Math.round(Number(e.target.value) || 0))))}
          style={{ width: 70 }}
        />
      </label>
      {err && <div className="setting-desc">{err}</div>}
    </div>
  );
}
