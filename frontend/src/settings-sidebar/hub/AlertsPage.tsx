import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProviderLogo } from "../../notify-toast/ProviderLogo";
import { statusLabel } from "../../notify-toast/notifyToastModel";
import { SwitchCard } from "../../theme/OptionCard";
import {
  DEFAULT_ALERT_SETTINGS,
  groupByDay,
  normalizeAlert,
  normalizePrivacy,
  privacyExplanation,
  privacyLabel,
  relativeTime,
  type AlertItem,
  type AlertSettingKey,
  type AlertSettings,
  type NotifyPrivacy,
} from "./alertsModel";

export interface AlertsPageProps {
  initialAlerts?: AlertItem[];
  initialSettings?: Partial<AlertSettings>;
  initialLoading?: boolean;
  initialError?: string | null;
  onSettingChanged?: (key: AlertSettingKey, value: unknown) => void;
}

const PRIVACY_MODES: NotifyPrivacy[] = ["normal", "private", "hidden"];

export function AlertsPage({
  initialAlerts,
  initialSettings,
  initialLoading,
  initialError,
  onSettingChanged,
}: AlertsPageProps = {}) {
  const [settings, setSettings] = useState<AlertSettings>(() => ({
    ...DEFAULT_ALERT_SETTINGS,
    ...initialSettings,
  }));
  const [alerts, setAlerts] = useState<AlertItem[]>(() => initialAlerts ?? []);
  const [loading, setLoading] = useState<boolean>(() => initialLoading ?? (initialAlerts === undefined));
  const [error, setError] = useState<string | null>(() => initialError ?? null);
  const [busy, setBusy] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [now, setNow] = useState(() => Date.now());

  // Load settings from backend
  const loadSettings = useCallback(async () => {
    try {
      const s = await invoke<Record<string, unknown>>("get_settings");
      if (s) {
        setSettings({
          notifyToast: typeof s.notifyToast === "boolean" ? s.notifyToast : true,
          notifyNative: typeof s.notifyNative === "boolean" ? s.notifyNative : true,
          notifyUrgent: typeof s.notifyUrgent === "boolean" ? s.notifyUrgent : true,
          notifyPrivacy: normalizePrivacy(s.notifyPrivacy),
        });
      }
    } catch {
      // Retain defaults if get_settings fails
    }
  }, []);

  // Fetch recent alerts list
  const fetchAlerts = useCallback(async () => {
    try {
      const raw = await invoke<unknown[]>("notify_list_events");
      const normalized: AlertItem[] = [];
      if (Array.isArray(raw)) {
        for (const item of raw) {
          const a = normalizeAlert(item);
          if (a) normalized.push(a);
        }
      }
      setAlerts(normalized);
      setError(null);
      setNow(Date.now());
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  // Persist a single setting change
  const saveSetting = async <K extends keyof AlertSettings>(key: K, value: AlertSettings[K]) => {
    setBusy(true);
    try {
      // Patch only this setting: writing the whole (possibly stale) settings object back could undo a change made elsewhere.
      await invoke("settings_patch", { patch: { [key]: value } });
      setSettings((prev) => ({ ...prev, [key]: value }));
      onSettingChanged?.(key, value);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  // Open an alert's sidebar detail
  const handleOpen = async (eventId: string) => {
    try {
      await invoke("notify_alert_open", { eventId });
    } catch (err) {
      setError(String(err));
    }
  };

  // Clear all alerts
  const handleClear = async () => {
    setClearing(true);
    try {
      await invoke("notify_alerts_clear");
      setConfirmClear(false);
      await fetchAlerts();
    } catch (err) {
      setError(String(err));
    } finally {
      setClearing(false);
    }
  };

  // Initial load and periodic refresh (every 5s and on window focus)
  useEffect(() => {
    void loadSettings();
    void fetchAlerts();

    const interval = setInterval(() => {
      void fetchAlerts();
    }, 5000);

    const onFocus = () => {
      void fetchAlerts();
    };
    window.addEventListener("focus", onFocus);

    return () => {
      clearInterval(interval);
      window.removeEventListener("focus", onFocus);
    };
  }, [loadSettings, fetchAlerts]);

  const groups = groupByDay(alerts, now);

  return (
    <div className="hx-page">
      {/* ── Error Banner ── */}
      {error && (
        <div className="hx-alerts-error" role="alert">
          <span className="hx-alerts-error-msg">{error}</span>
          <button
            type="button"
            className="hx-btn hx-btn--tiny"
            onClick={() => {
              setError(null);
              void fetchAlerts();
            }}
          >
            Retry
          </button>
        </div>
      )}

      {/* ── Top Notification Switches ── */}
      <div className="hx-alerts-switches">
        <SwitchCard
          icon="🔔"
          title="NEXUS Toast"
          desc="Floating notifications on screen for important events."
          checked={settings.notifyToast}
          disabled={busy}
          onChange={(val) => void saveSetting("notifyToast", val)}
        />
        <SwitchCard
          icon="🪟"
          title="Windows Notification Fallback"
          desc="Send alerts to the Windows Action Center when NEXUS is in the background."
          checked={settings.notifyNative}
          disabled={busy}
          onChange={(val) => void saveSetting("notifyNative", val)}
        />
        <SwitchCard
          icon="🚨"
          title="Pass Do Not Disturb for Failures"
          desc="Always show failed build and deploy alerts even when Focus Assist or DND is active."
          checked={settings.notifyUrgent}
          disabled={busy}
          onChange={(val) => void saveSetting("notifyUrgent", val)}
        />
      </div>

      {/* ── Privacy Segmented Control ── */}
      <div className="hx-alerts-privacy-card">
        <div className="hx-alerts-privacy-header">
          <span className="hx-alerts-privacy-title">Alert Privacy</span>
        </div>
        <div className="hx-tab-nav" role="radiogroup" aria-label="Alert privacy mode">
          {PRIVACY_MODES.map((mode) => (
            <button
              key={mode}
              type="button"
              role="radio"
              aria-checked={settings.notifyPrivacy === mode}
              className={`hx-tab-btn ${settings.notifyPrivacy === mode ? "is-active" : ""}`}
              disabled={busy}
              onClick={() => void saveSetting("notifyPrivacy", mode)}
            >
              {privacyLabel(mode)}
            </button>
          ))}
        </div>
        <div className="hx-alerts-privacy-desc">
          {privacyExplanation(settings.notifyPrivacy)}
        </div>
      </div>

      <div className="hx-divider" style={{ margin: "16px 0" }} />

      {/* ── Alerts List Header & Clear Action ── */}
      <div className="hx-alerts-header">
        <span className="hx-alerts-count-label">
          {`Recent Alerts (${alerts.length})`}
        </span>
        {confirmClear ? (
          <div className="hx-confirm-inline">
            <span className="hx-confirm-msg">Clear all?</span>
            <button
              type="button"
              className="hx-btn hx-btn--tiny"
              onClick={() => setConfirmClear(false)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="hx-btn hx-btn--tiny hx-btn--danger"
              disabled={clearing}
              onClick={() => void handleClear()}
            >
              {clearing ? "Clearing…" : "Clear all"}
            </button>
          </div>
        ) : (
          <button
            type="button"
            className="hx-btn hx-btn--tiny"
            disabled={alerts.length === 0}
            onClick={() => setConfirmClear(true)}
            aria-label="Clear all alerts"
          >
            Clear
          </button>
        )}
      </div>

      {/* ── Non-blocking Loading State ── */}
      {loading && alerts.length === 0 && !error && (
        <div className="hx-note">Loading alerts…</div>
      )}

      {/* ── Empty State ── */}
      {!loading && alerts.length === 0 && !error && (
        <div className="hx-empty">No alerts yet.</div>
      )}

      {/* ── Grouped Alert Rows ── */}
      {groups.map((group) => (
        <div key={group.day} className="hx-alerts-group">
          <div className="hx-alerts-group-header">{group.day}</div>
          <div className="hx-alerts-list" role="list">
            {group.items.map((alert) => (
              <button
                key={alert.eventId}
                type="button"
                className="hx-alert-row"
                onClick={() => void handleOpen(alert.eventId)}
                aria-label={`${alert.provider}: ${alert.title}`}
              >
                {!alert.read && (
                  <span className="hx-alert-unread-dot" title="Unread" aria-label="Unread" />
                )}
                <ProviderLogo icon={alert.icon} provider={alert.provider} />
                <div className="hx-alert-meta">
                  <div className="hx-alert-row-top">
                    <span className="hx-alert-title">{alert.title}</span>
                    {alert.count > 1 && (
                      <span className="hx-alert-count">{`+${alert.count}`}</span>
                    )}
                    <span className={`hx-alert-status hx-alert-status--${alert.status}`}>
                      {statusLabel(alert.status)}
                    </span>
                    <span className="hx-alert-time">{relativeTime(alert.updatedAtMs, now)}</span>
                  </div>
                  {alert.preview ? (
                    <div className="hx-alert-preview">{alert.preview}</div>
                  ) : null}
                </div>
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
