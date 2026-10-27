import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { workerJson } from "../../net/workerApi";

type Binding = { phone: string; verified_at: number } | null;
type Pending = { request_id: string; source_device_id: string; device_name: string | null; expires_at: number };
type Link = { link_id: string; device_a: string; device_b: string; device_name: string | null; created_at: number };
type Activity = {
  supported: boolean;
  can_clear: boolean;
  enabled: boolean;
  sharing_available: boolean;
  days: Array<{ day: string; active_minutes: number; apps: Array<{ app: string; minutes: number }> }>;
};

/**
 * Device Links & Hardware Mesh:
 * Direct, verified laptop-to-laptop pairing. No Administrator privilege is exposed.
 */
export function DeviceLinksPage({ onOpenActivity }: { onOpenActivity?: () => void } = {}) {
  const [binding, setBinding] = useState<Binding>(null);
  const [phone, setPhone] = useState("");
  const [challengeId, setChallengeId] = useState("");
  const [smsCode, setSmsCode] = useState("");
  const [targetPhone, setTargetPhone] = useState("");
  const [outgoingCode, setOutgoingCode] = useState("");
  const [pending, setPending] = useState<Pending[]>([]);
  const [links, setLinks] = useState<Link[]>([]);
  const [codes, setCodes] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [activity, setActivity] = useState<Activity | null>(null);
  const [activityError, setActivityError] = useState("");

  const refresh = useCallback(async () => {
    const [p, q, l] = await Promise.all([
      workerJson<{ binding: Binding }>("GET", "/v1/links/phone"),
      workerJson<{ requests: Pending[] }>("GET", "/v1/links/pending"),
      workerJson<{ links: Link[] }>("GET", "/v1/links/list"),
    ]);
    setBinding(p.binding);
    setPending(q.requests);
    setLinks(l.links);
  }, []);

  useEffect(() => {
    refresh().catch((e) => setError(String(e)));
  }, [refresh]);
  useEffect(() => {
    invoke<Activity>("device_activity_status").then(setActivity)
      .catch((e) => setActivityError(String(e)));
  }, []);

  const act = async (task: () => Promise<void>) => {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await task();
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="hx-page">
      {error && <div className="hx-error" role="alert" onClick={() => setError("")}>{error}</div>}
      {message && <div className="hx-note" role="status">{message}</div>}
      <div className="hx-note">Pairing is available here. File transfer and activity sharing are not available yet. Pairing never grants Administrator access.</div>

      {/* ── Verified Mobile Identity ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <div className="hx-switch-title">
            <span>Verified Phone</span>
            {binding && (
              <span className="hx-pill hx-pill--on" style={{ fontSize: 10, background: "rgba(16, 185, 129, 0.2)", color: "#34d399" }}>
                ✓ Verified
              </span>
            )}
          </div>
        </div>

        {binding ? (
          <div style={{ marginTop: 12, padding: "12px 14px", background: "rgba(0, 0, 0, 0.25)", borderRadius: 10, display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <div>
              <div style={{ fontSize: 14, fontWeight: 700, color: "#fff", letterSpacing: "0.02em" }}>
                📱 {binding.phone}
              </div>
              <div className="hx-fact-meta" style={{ marginTop: 3 }}>
                Binding active • Verified on this hardware
              </div>
            </div>
            <button
              className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
              disabled={busy}
              onClick={() =>
                act(async () => {
                  await workerJson("DELETE", "/v1/links/phone");
                  setMessage("Phone binding removed. Existing links remain active until revoked.");
                })
              }
            >
              Unlink Number
            </button>
          </div>
        ) : (
          <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 10 }}>
            <div style={{ display: "flex", gap: 8 }}>
              <input
                aria-label="Your phone number"
                type="tel"
                className="hx-input"
                placeholder="+1 (555) 000-0000 or +91 98765 43210"
                value={phone}
                onChange={(e) => setPhone(e.target.value)}
                style={{ flex: 1 }}
              />
              <button
                className="hx-btn hx-btn--primary"
                disabled={busy || !/^\+[1-9][0-9]{7,14}$/.test(phone.replace(/\s+/g, ""))}
                onClick={() =>
                  act(async () => {
                    const result = await workerJson<{ challenge_id: string }>("POST", "/v1/links/phone/start", {
                      phone: phone.replace(/\s+/g, ""),
                    });
                    setChallengeId(result.challenge_id);
                    setMessage("Verification code sent via SMS. Enter it below.");
                  })
                }
              >
                Send SMS Code
              </button>
            </div>

            {challengeId && (
              <div style={{ marginTop: 8, padding: 12, background: "rgba(0, 0, 0, 0.3)", borderRadius: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                <div style={{ fontSize: 12, fontWeight: 600, color: "#fff" }}>Enter 6-Digit SMS Code:</div>
                <div style={{ display: "flex", gap: 8 }}>
                  <input
                    aria-label="SMS verification code"
                    className="hx-input"
                    inputMode="numeric"
                    maxLength={6}
                    placeholder="123456"
                    value={smsCode}
                    onChange={(e) => setSmsCode(e.target.value.replace(/\D/g, ""))}
                    style={{ flex: 1, letterSpacing: "0.2em", fontSize: 16, textAlign: "center" }}
                  />
                  <button
                    className="hx-btn hx-btn--primary"
                    disabled={busy || smsCode.length !== 6}
                    onClick={() =>
                      act(async () => {
                        await workerJson("POST", "/v1/links/phone/confirm", {
                          challenge_id: challengeId,
                          code: smsCode,
                        });
                        setSmsCode("");
                        setChallengeId("");
                        setMessage("Phone verified successfully.");
                      })
                    }
                  >
                    Confirm
                  </button>
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* ── Connect Another Laptop ── */}
      {binding && (
        <div className="hx-card" style={{ padding: "12px 14px" }}>
          <div className="hx-switch-title">Pair Another Laptop</div>

          <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
            <input
              aria-label="Target verified phone number"
              type="tel"
              className="hx-input"
              placeholder="+1 555 123 4567"
              value={targetPhone}
              onChange={(e) => setTargetPhone(e.target.value)}
              style={{ flex: 1 }}
            />
            <button
              className="hx-btn hx-btn--primary"
              disabled={busy || !/^\+[1-9][0-9]{7,14}$/.test(targetPhone.replace(/\s+/g, ""))}
              onClick={() =>
                act(async () => {
                  const result = await workerJson<{ code: string }>("POST", "/v1/links/request", {
                    phone: targetPhone.replace(/\s+/g, ""),
                  });
                  setOutgoingCode(result.code);
                  setMessage("Pairing key generated.");
                })
              }
            >
              Generate Key
            </button>
          </div>

          {outgoingCode && (
            <div style={{ marginTop: 10, padding: 10, background: "rgba(59, 130, 246, 0.1)", border: "1px solid rgba(59, 130, 246, 0.3)", borderRadius: 8, textAlign: "center" }}>
              <div style={{ fontSize: 10.5, fontWeight: 600, color: "#93c5fd", textTransform: "uppercase" }}>
                10-Minute Pairing Key
              </div>
              <div style={{ fontSize: 24, fontWeight: 800, letterSpacing: "0.2em", color: "#ffffff", margin: "4px 0" }}>
                {outgoingCode}
              </div>
            </div>
          )}
        </div>
      )}

      {/* ── Incoming Pairing Requests ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <div className="hx-switch-title">Incoming Handshakes ({pending.length})</div>
          <button className="hx-btn hx-btn--tiny" disabled={busy} onClick={() => act(refresh)}>
            Refresh
          </button>
        </div>

        {pending.length === 0 ? (
          <div style={{ padding: "16px 0", textAlign: "center", color: "rgba(255, 255, 255, 0.45)", fontSize: 12 }}>
            No incoming pairing requests at this moment.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 10 }}>
            {pending.map((request) => (
              <div
                key={request.request_id}
                style={{
                  padding: 12,
                  background: "rgba(0, 0, 0, 0.25)",
                  border: "1px solid rgba(255, 255, 255, 0.08)",
                  borderRadius: 10,
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                }}
              >
                <div>
                  <div style={{ fontSize: 13, fontWeight: 600, color: "#fff" }}>
                    💻 {request.device_name || "Another NEXUS Laptop"}
                  </div>
                  <div className="hx-fact-meta">Requests device pairing</div>
                </div>

                <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                  <input
                    aria-label={`Code for ${request.device_name || "request"}`}
                    className="hx-input"
                    inputMode="numeric"
                    maxLength={6}
                    placeholder="Code"
                    value={codes[request.request_id] || ""}
                    onChange={(e) =>
                      setCodes((prev) => ({
                        ...prev,
                        [request.request_id]: e.target.value.replace(/\D/g, ""),
                      }))
                    }
                    style={{ width: "90px", textAlign: "center", letterSpacing: "0.1em" }}
                  />
                  <button
                    className="hx-btn hx-btn--primary hx-btn--tiny"
                    disabled={busy || (codes[request.request_id] || "").length !== 6}
                    onClick={() =>
                      act(async () => {
                        await workerJson("POST", "/v1/links/accept", {
                          request_id: request.request_id,
                          code: codes[request.request_id],
                        });
                        setMessage("Laptops paired. File transfer is not available yet.");
                      })
                    }
                  >
                    Accept Link
                  </button>
                  <button
                    className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                    disabled={busy}
                    onClick={() => act(async () => {
                      await workerJson("POST", "/v1/links/reject", { request_id: request.request_id });
                      setMessage("Pairing request declined.");
                    })}
                  >
                    Decline
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* ── Linked Hardware Mesh ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div className="hx-switch-title">Linked Laptops ({links.length})</div>

        {links.length === 0 ? (
          <div style={{ padding: "12px 0", textAlign: "center", color: "rgba(255, 255, 255, 0.45)", fontSize: 12 }}>
            No laptops paired yet.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 10 }}>
            {links.map((link) => (
              <div
                key={link.link_id}
                style={{
                  padding: "10px 14px",
                  background: "rgba(0, 0, 0, 0.2)",
                  border: "1px solid rgba(255, 255, 255, 0.06)",
                  borderRadius: 10,
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <span style={{ fontSize: 20 }}>💻</span>
                  <div>
                    <div style={{ fontSize: 13, fontWeight: 600, color: "#fff" }}>
                      {link.device_name || "Linked NEXUS Laptop"}
                    </div>
                    <div className="hx-fact-meta">
                      ID: {link.link_id} • Linked {new Date(link.created_at).toLocaleDateString()}
                    </div>
                  </div>
                </div>
                <button
                  className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                  disabled={busy}
                  onClick={() =>
                    act(async () => {
                      await workerJson("POST", "/v1/links/revoke", { link_id: link.link_id });
                      setMessage("Device connection revoked.");
                    })
                  }
                >
                  Disconnect
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div className="hx-switch-title">Activity Manager</div>
        {onOpenActivity && <button className="hx-btn hx-btn--tiny" onClick={onOpenActivity}>Open activity dashboard</button>}
        <p className="hx-fact-meta">Optional local history for 30 days. Only app names and estimated active minutes are recorded. Window titles, browser sites, and page URLs are excluded.</p>
        {activityError && <div className="hx-error" role="alert">{activityError}</div>}
        {activity && (
          <>
            {activity.supported ? (
              <label style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 10 }}>
                <input
                  type="checkbox"
                  checked={activity.enabled}
                  disabled={busy}
                  onChange={async (e) => {
                    setBusy(true);
                    setActivityError("");
                    try {
                      setActivity(await invoke<Activity>("device_activity_set_enabled", { enabled: e.target.checked }));
                    } catch (error) {
                      setActivityError(String(error));
                    } finally {
                      setBusy(false);
                    }
                  }}
                />
                Record app activity locally
              </label>
            ) : (
              <p className="hx-fact-meta">Recording is unavailable. It needs Windows foreground-app support and an OS encryption key. If stored history is damaged, clear it to reset.</p>
            )}
            <p className="hx-fact-meta">Sharing activity with linked laptops is unavailable until the separate consent and encrypted transfer flow is ready.</p>
            {activity.can_clear && (
              <button
                className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                disabled={busy}
                onClick={async () => {
                  setBusy(true);
                  setActivityError("");
                  try {
                    setActivity(await invoke<Activity>("device_activity_clear"));
                  } catch (error) {
                    setActivityError(String(error));
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                Clear local activity history
              </button>
            )}
            {activity.days.length === 0 && <p className="hx-fact-meta">No activity recorded.</p>}
            {activity.days.map((day) => (
              <div key={day.day} style={{ marginTop: 10 }}>
                <div style={{ fontSize: 12, fontWeight: 700 }}>{day.day} · about {day.active_minutes} active minutes</div>
                <ul>{day.apps.map((app) => <li key={app.app}>{app.app}: {app.minutes} min</li>)}</ul>
              </div>
            ))}
          </>
        )}
      </div>
    </div>
  );
}
