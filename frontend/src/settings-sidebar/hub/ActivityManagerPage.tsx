import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import activityIconColors from "./activityIconColors.json";
import "./activity-manager.css";

type AppMinutes = { app: string; minutes: number; seconds?: number };
type AppTime = { app: string; seconds: number };
type DayActivity = { day: string; active_minutes: number; apps: AppMinutes[] };
type DaySites = { day: string; sites: { domain: string; seconds: number }[] };
type ActivityStatus = {
  supported: boolean;
  can_clear: boolean;
  enabled: boolean;
  sites_enabled: boolean;
  site_days: DaySites[];
  browser_connected: boolean;
  days: DayActivity[];
};
type Period = "today" | "week" | "month";

const BRAND: Record<string, { color: string; icon?: string }> = {
  Brave: { color: "#ed6b39", icon: "brave" },
  Chrome: { color: "#4285f4", icon: "chrome" },
  Claude: { color: "#d97757", icon: "claude" },
  Cursor: { color: "#262626", icon: "cursor" },
  Discord: { color: "#5865f2", icon: "discord" },
  Edge: { color: "#1aa1b8", icon: "edge" },
  Excel: { color: "#217346", icon: "excel" },
  "File Explorer": { color: "#e8ac2f", icon: "fileexplorer" },
  Firefox: { color: "#e95f36", icon: "firefox" },
  Instagram: { color: "#e9389e", icon: "instagram" },
  Notepad: { color: "#397acf", icon: "notepad" },
  OpenCode: { color: "#3d3d50", icon: "opencode" },
  Postman: { color: "#ff6c37", icon: "postman" },
  PowerPoint: { color: "#d24726", icon: "powerpoint" },
  Slack: { color: "#4a154b", icon: "slack" },
  Spotify: { color: "#1db954", icon: "spotify" },
  Teams: { color: "#6264a7", icon: "teams" },
  Terminal: { color: "#2f3543", icon: "terminal" },
  TikTok: { color: "#ee2958", icon: "tiktok" },
  Twitter: { color: "#171717", icon: "twitter" },
  "VS Code": { color: "#1476c8", icon: "vscode" },
  "Visual Studio": { color: "#6d43a8", icon: "visualstudio" },
  VLC: { color: "#ff8800", icon: "vlc" },
  WhatsApp: { color: "#25d366", icon: "whatsapp" },
  Word: { color: "#2b579a", icon: "word" },
  YouTube: { color: "#ff0033", icon: "youtube" },
  Zoom: { color: "#2d8cff", icon: "zoom" },
};

const ICON_ALIASES: Record<string, string> = {
  balenaetcher: "balena", blackmagicraw: "blackmagic",
  blenderlauncher: "blender", chromeexe: "chrome",
  cloudapplicationplatformrender: "render", codexapp: "codex", code: "vscode",
  davinciresolve: "davinci", githubdesktop: "github", googlechrome: "chrome",
  googlecloudconsole: "googlecloud", googlegemini: "gemini",
  microsoftclipchamp: "clipchamp", microsoftedge: "edge",
  microsoftteams: "teams", mongodbcompass: "mongodb",
  msteams: "teams", olk: "outlook", powertoysquickaccess: "powertoys",
  pwsh: "powershell", qbittorrent: "qbittorrent", resolve: "davinci",
  studio64: "androidstudio", supabasestudio: "supabase",
  telegramdesktop: "telegram", visualstudiocode: "vscode",
  whatsapproot: "whatsapp", windowspowershell: "powershell",
  windowsterminal: "terminal",
};
const ICON_COLORS = activityIconColors as Record<string, string>;
const SITE_ICONS: Record<string, string> = {
  "youtube.com": "youtube", "discord.com": "discord", "spotify.com": "spotify",
  "instagram.com": "instagram", "tiktok.com": "tiktok", "x.com": "twitter",
  "twitter.com": "twitter", "github.com": "github", "figma.com": "figma",
  "slack.com": "slack", "whatsapp.com": "whatsapp", "netflix.com": "netflix",
  "reddit.com": "reddit", "notion.so": "notion", "openai.com": "openai",
  "chatgpt.com": "chatgpt", "claude.ai": "claude", "google.com": "google",
  "gmail.com": "gmail", "mail.google.com": "gmail", "drive.google.com": "googledrive",
};
const ICON_ASSETS = new Set([
  ...Object.keys(ICON_COLORS),
  ...Object.values(BRAND).flatMap((brand) => brand.icon ? [brand.icon] : []),
  "balena", "genericapp",
]);

function iconOf(app: string): string {
  const normalized = app.toLowerCase().replace(/[^a-z0-9]/g, "");
  const domain = app.toLowerCase();
  const site = Object.keys(SITE_ICONS).sort((a, b) => b.length - a.length)
    .find((key) => domain === key || domain.endsWith(`.${key}`));
  const hostLabel = domain.split(".").at(-2);
  const siteAsset = site ? SITE_ICONS[site] : hostLabel && ICON_ASSETS.has(hostLabel) ? hostLabel : undefined;
  const candidate = siteAsset ?? BRAND[app]?.icon ?? ICON_ALIASES[normalized] ?? normalized;
  return candidate && ICON_ASSETS.has(candidate) ? candidate : "genericapp";
}

function darkEnoughForWhiteText(hex: string): string {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) return "#4f7df3";
  const rgb = [1, 3, 5].map((start) => parseInt(hex.slice(start, start + 2), 16));
  const luminance = () => rgb.map((channel) => {
    const value = channel / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  }).reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
  while (luminance() > 0.183) for (let index = 0; index < 3; index++) rgb[index] = Math.floor(rgb[index] * 0.85);
  return `#${rgb.map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

function colorOf(app: string): string {
  const asset = iconOf(app);
  const color = BRAND[app]?.color
    ?? Object.values(BRAND).find((brand) => brand.icon === asset)?.color
    ?? ICON_COLORS[asset];
  if (color) return darkEnoughForWhiteText(color);
  const palette = ["#4f7df3", "#ec5f8f", "#ed9837", "#21b8a7", "#9569e7", "#27a9d7"];
  let hash = 0;
  for (const char of app) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return palette[(hash >>> 0) % palette.length];
}

function Icon({ app }: { app: string }) {
  const [failed, setFailed] = useState(false);
  const asset = iconOf(app);
  useEffect(() => setFailed(false), [app]);
  return (
    <span className="am-icon" style={{ color: colorOf(app) }} aria-hidden="true">
      {asset && !failed ? (
        <img src={`/activity-logos/${asset}.svg`} alt="" onError={() => setFailed(true)} />
      ) : (
        <span>{app.slice(0, 1).toUpperCase()}</span>
      )}
    </span>
  );
}

function minutesLabel(seconds: number): string {
  if (seconds > 0 && seconds < 60) return "<1 min";
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return hours ? `${hours} h ${rest} min` : `${rest} min`;
}

function localDay(offset = 0): string {
  const day = new Date();
  day.setDate(day.getDate() + offset);
  return `${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, "0")}-${String(day.getDate()).padStart(2, "0")}`;
}

function aggregate(days: DayActivity[]): AppTime[] {
  const totals = new Map<string, number>();
  for (const day of days) for (const item of day.apps) {
    totals.set(item.app, (totals.get(item.app) ?? 0) + (item.seconds ?? item.minutes * 60));
  }
  return [...totals].map(([app, seconds]) => ({ app, seconds }))
    .sort((a, b) => b.seconds - a.seconds || a.app.localeCompare(b.app));
}

export function ActivityManagerPage() {
  const [status, setStatus] = useState<ActivityStatus | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [period, setPeriod] = useState<Period>("today");
  const [selectedApp, setSelectedApp] = useState<string | null>(null);
  const [selectedSite, setSelectedSite] = useState<string | null>(null);
  const [browserToken, setBrowserToken] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  const refreshStatus = async () => {
    setError("");
    try { setStatus(await invoke<ActivityStatus>("device_activity_status")); }
    catch (e) { setError(String(e)); }
  };

  useEffect(() => {
    void refreshStatus();
    const timer = window.setInterval(() => {
      invoke<ActivityStatus>("device_activity_status").then(setStatus).catch(() => {});
    }, 30_000);
    return () => window.clearInterval(timer);
  }, []);

  const days = useMemo(() => {
    const cutoff = localDay(period === "today" ? 0 : period === "week" ? -6 : -29);
    return status?.days.filter((day) => day.day >= cutoff && day.day <= localDay()) ?? [];
  }, [status, period]);
  const apps = useMemo(() => aggregate(days), [days]);
  const sites = useMemo(() => {
    const cutoff = localDay(period === "today" ? 0 : period === "week" ? -6 : -29);
    const totals = new Map<string, number>();
    for (const day of status?.site_days ?? []) {
      if (day.day < cutoff || day.day > localDay()) continue;
      for (const site of day.sites) totals.set(site.domain, (totals.get(site.domain) ?? 0) + site.seconds);
    }
    return [...totals].map(([app, seconds]) => ({ app, seconds }))
      .sort((a, b) => b.seconds - a.seconds || a.app.localeCompare(b.app));
  }, [status, period]);
  const total = apps.reduce((sum, app) => sum + app.seconds, 0);
  const top = apps.slice(0, 3);
  const segments = apps.slice(0, 5);
  const segmentTotal = segments.reduce((sum, app) => sum + app.seconds, 0);
  let cursor = 0;
  const gradient = segments.map((item) => {
    const start = cursor;
    cursor += total ? item.seconds / total * 100 : 0;
    return `${colorOf(item.app)} ${start}% ${cursor}%`;
  });
  if (segmentTotal < total) gradient.push(`#32323b ${cursor}% 100%`);
  const selectedDays = selectedApp
    ? days.map((day) => {
        const item = day.apps.find((app) => app.app === selectedApp);
        return { day: day.day, seconds: item?.seconds ?? (item?.minutes ?? 0) * 60 };
      }).filter((day) => day.seconds > 0)
    : [];

  const changeEnabled = async (enabled: boolean) => {
    setBusy(true);
    setError("");
    try {
      setStatus(await invoke<ActivityStatus>("device_activity_set_enabled", { enabled }));
      if (!enabled) setBrowserToken(null);
    }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };

  const changeSitesEnabled = async (enabled: boolean) => {
    setBusy(true);
    setError("");
    try {
      setStatus(await invoke<ActivityStatus>("device_activity_set_sites_enabled", { enabled }));
      if (!enabled) setBrowserToken(null);
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };

  const revealBrowserToken = async () => {
    setError("");
    try { setBrowserToken(await invoke<string>("device_activity_browser_token")); }
    catch (e) { setError(String(e)); }
  };

  const clearHistory = async () => {
    setBusy(true);
    setError("");
    try {
      setStatus(await invoke<ActivityStatus>("device_activity_clear"));
      setSelectedApp(null);
      setSelectedSite(null);
      setConfirmClear(false);
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div className="hx-page am-page">
      <div className="am-head">
        <div>
          <div className="am-eyebrow">ACTIVITY MANAGER</div>
          <h2>{selectedApp ?? selectedSite ?? "Your activity"}</h2>
        </div>
        {(selectedApp || selectedSite) && <button className="am-text-button" onClick={() => { setSelectedApp(null); setSelectedSite(null); }}>Overview</button>}
      </div>

      {error && <div className="hx-error" role="alert">{error}</div>}
      {!status ? <div className="hx-card am-empty">
        {error ? "Could not load activity." : "Loading activity…"}
        {error && <button className="am-text-button" onClick={() => void refreshStatus()}>Retry</button>}
      </div> : !status.supported ? (
        <div className="hx-card am-empty">
          <p>Activity recording is unavailable on this device, or the stored history is unreadable.</p>
          {status.can_clear && <>
            <p>If stored history is damaged, clear it to reset recording.</p>
            {confirmClear ? <div className="am-clear-confirm">
              <button onClick={() => setConfirmClear(false)}>Cancel</button>
              <button disabled={busy} onClick={() => void clearHistory()}>Delete history</button>
            </div> : <button className="am-text-button" onClick={() => setConfirmClear(true)}>Clear local history</button>}
          </>}
        </div>
      ) : (
        <>
          <div className="am-periods" role="group" aria-label="Activity period">
            {(["today", "week", "month"] as Period[]).map((value) => (
              <button key={value} className={period === value ? "am-active" : ""} onClick={() => setPeriod(value)}>
                {value === "today" ? "Today" : value === "week" ? "7 days" : "30 days"}
              </button>
            ))}
          </div>

          {selectedSite ? (
            <>
              <div className="am-hero" style={{ background: colorOf(selectedSite) }}>
                <Icon app={selectedSite} /><strong>{selectedSite}</strong>
                <span>{minutesLabel(sites.find((site) => site.app === selectedSite)?.seconds ?? 0)} in this period</span>
              </div>
              <h3 className="am-section-title">Daily website history</h3>
              {(status.site_days ?? []).filter((day) => days.some((appDay) => appDay.day === day.day))
                .map((day) => ({ day: day.day, seconds: day.sites.find((site) => site.domain === selectedSite)?.seconds ?? 0 }))
                .filter((day) => day.seconds > 0).map((day) => (
                  <div className="am-history-row" key={day.day}><span>{day.day}</span><strong>{minutesLabel(day.seconds)}</strong></div>
                ))}
            </>
          ) : selectedApp ? (
            <>
              <div className="am-hero" style={{ background: colorOf(selectedApp) }}>
                <Icon app={selectedApp} />
                <strong>{selectedApp}</strong>
                <span>{minutesLabel(apps.find((item) => item.app === selectedApp)?.seconds ?? 0)} in this period</span>
              </div>
              <h3 className="am-section-title">Daily history</h3>
              {selectedDays.length ? selectedDays.map((day) => (
                <div className="am-history-row" key={day.day}>
                  <span>{day.day}</span><strong>{minutesLabel(day.seconds)}</strong>
                </div>
              )) : <div className="hx-card am-empty">No recorded activity for this period.</div>}
            </>
          ) : (
            <>
              <div className="am-topline"><span>Active time</span><strong>{minutesLabel(total)}</strong></div>
              {total > 0 ? (
                <>
                  <div className="am-shares" aria-label="Top app shares">
                    {top.map((item) => (
                      <button key={item.app} style={{ background: colorOf(item.app) }} onClick={() => setSelectedApp(item.app)} title={`${item.app}: ${Math.round(item.seconds / total * 100)}%`}>
                        <Icon app={item.app} /> {Math.round(item.seconds / total * 100)}%
                      </button>
                    ))}
                  </div>
                  <div className="am-tiles">
                    {top.map((item) => (
                      <button className="am-tile" key={item.app} style={{ background: colorOf(item.app) }} onClick={() => setSelectedApp(item.app)}>
                        <Icon app={item.app} />
                        <strong>{item.app}</strong>
                        <span>{minutesLabel(item.seconds)}</span>
                      </button>
                    ))}
                  </div>
                  <div className="hx-card am-breakdown">
                    <div className="am-ring" style={{ background: `conic-gradient(${gradient.join(", ")})` }} role="img" aria-label={`Activity breakdown: ${apps.map((item) => `${item.app} ${minutesLabel(item.seconds)}`).join(", ")}`}>
                      <div><strong>{minutesLabel(total)}</strong><small>active time</small></div>
                    </div>
                    <div className="am-legend">
                      {apps.map((item) => (
                        <button key={item.app} onClick={() => setSelectedApp(item.app)}>
                          <Icon app={item.app} /><span>{item.app}</span><strong>{minutesLabel(item.seconds)}</strong>
                        </button>
                      ))}
                    </div>
                  </div>
                  {(status.sites_enabled || sites.length > 0) && <div className="hx-card am-sites">
                    <h3>Websites <small>within browser time</small></h3>
                    {sites.length ? sites.map((site) => (
                      <button key={site.app} onClick={() => setSelectedSite(site.app)}>
                        <Icon app={site.app} /><span>{site.app}</span><strong>{minutesLabel(site.seconds)}</strong>
                      </button>
                    )) : <p>No website time recorded for this period.</p>}
                  </div>}
                </>
              ) : (
                <div className="hx-card am-empty">{status.enabled ? "No activity recorded for this period yet." : "Turn on local recording to see your app activity here."}</div>
              )}
            </>
          )}

          <div className="hx-card am-controls">
            <label><span>Record app activity locally</span><input type="checkbox" checked={status.enabled} disabled={busy} onChange={(e) => void changeEnabled(e.target.checked)} /></label>
            <p>Estimated foreground-app time, encrypted on this device for 30 days.</p>
            <label><span>Record website domains separately</span><input type="checkbox" checked={status.sites_enabled} disabled={busy || !status.enabled} onChange={(e) => void changeSitesEnabled(e.target.checked)} /></label>
            <p>Requires the Nexus browser extension in Chrome, Brave, or Edge. Only the focused normal tab’s domain is stored; private windows, page URLs, titles, paths and queries are excluded. Website time is already included in browser app time.</p>
            {status.sites_enabled && <div className="am-browser-setup">
              <span>{status.browser_connected ? "Browser signal active" : "Waiting for a browser signal"}</span>
              <button className="am-text-button" onClick={() => void revealBrowserToken()}>Show browser connection code</button>
              <button className="am-text-button" onClick={() => void invoke("device_activity_open_extension_folder").catch((e) => setError(String(e)))}>Open extension folder</button>
              {browserToken && <code>{browserToken}</code>}
              <small>In Chrome, Brave, or Edge extensions, enable Developer mode, choose Load unpacked, select this folder, then paste the code into its popup.</small>
            </div>}
            {status.can_clear && (confirmClear ? (
              <div className="am-clear-confirm"><button onClick={() => setConfirmClear(false)}>Cancel</button><button disabled={busy} onClick={() => void clearHistory()}>Delete history</button></div>
            ) : <button className="am-text-button" onClick={() => setConfirmClear(true)}>Clear activity history</button>)}
          </div>
        </>
      )}
    </div>
  );
}
