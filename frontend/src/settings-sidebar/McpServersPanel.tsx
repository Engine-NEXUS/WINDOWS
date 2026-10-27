import React, { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import { connectOAuth, disconnectOAuth, getOAuthStatus, setSidecarBaseUrl } from "../setup/oauth";
import { McpIcons } from "./McpIcons";
import "./mcpServers.css";

export type ConnectionState =
  | "connected"
  | "configured"
  | "stored"
  | "unsupported"
  | "local"
  | "partial"
  | "available"
  | "needs_setup"
  | "down"
  | "checking"
  | "error";

type ConnectionKind = "token" | "oauth" | "cli" | "bridge";

export type McpCategory =
  | "Cloud & Deployments"
  | "Developer & Engineering"
  | "Messaging & Notifications"
  | "Commerce & Everyday Apps";

export interface McpServerConfig {
  id: string;
  name: string;
  icon: keyof typeof McpIcons;
  category: McpCategory;
  kind: ConnectionKind;
  description?: string;
  vaultKey?: string;
  dashboardUrl?: string;
  loginUrl?: string;
  tokenUrl?: string;
  tokenPlaceholder?: string;
}

export const MCP_REGISTRY: McpServerConfig[] = [
  // ── Cloud & Deployments ──
  {
    id: "vercel",
    name: "Vercel",
    icon: "Vercel",
    category: "Cloud & Deployments",
    kind: "token",
    vaultKey: "vercel",
    description: "Inspect Next.js and edge deployments, trigger builds, and monitor preview aliases.",
    loginUrl: "https://vercel.com/login",
    tokenUrl: "https://vercel.com/account/tokens",
    tokenPlaceholder: "Vercel Access Token",
    dashboardUrl: "https://vercel.com/dashboard",
  },
  {
    id: "render",
    name: "Render",
    icon: "Render",
    category: "Cloud & Deployments",
    kind: "token",
    vaultKey: "render",
    description: "Query cloud web services, background workers, and trigger instant deploys.",
    loginUrl: "https://dashboard.render.com/login",
    tokenUrl: "https://dashboard.render.com/u/settings#api-keys",
    tokenPlaceholder: "rnd_... (Render API Key)",
    dashboardUrl: "https://dashboard.render.com",
  },
  {
    id: "netlify",
    name: "Netlify",
    icon: "Netlify",
    category: "Cloud & Deployments",
    kind: "token",
    vaultKey: "netlify",
    description: "Manage Jamstack edge deployments, trigger build webhooks, and read build logs.",
    loginUrl: "https://app.netlify.com/login",
    tokenUrl: "https://app.netlify.com/user/applications#personal-access-tokens",
    tokenPlaceholder: "Netlify Personal Access Token",
    dashboardUrl: "https://app.netlify.com",
  },
  {
    id: "supabase",
    name: "Supabase",
    icon: "Supabase",
    category: "Cloud & Deployments",
    kind: "token",
    vaultKey: "supabase",
    description: "Query PostgreSQL databases, monitor database health, and view migrations.",
    loginUrl: "https://supabase.com/dashboard",
    tokenUrl: "https://supabase.com/dashboard/account/tokens",
    tokenPlaceholder: "sbp_... (Supabase Access Token)",
    dashboardUrl: "https://supabase.com/dashboard",
  },

  // ── Developer & Engineering ──
  {
    id: "github",
    name: "GitHub",
    icon: "GitHub",
    category: "Developer & Engineering",
    kind: "oauth",
    description: "Inspect Pull Requests, manage code repositories, and automate issues.",
    loginUrl: "https://github.com/login",
    dashboardUrl: "https://github.com",
  },
  {
    id: "coderabbit",
    name: "CodeRabbit",
    icon: "CodeRabbit",
    category: "Developer & Engineering",
    kind: "cli",
    description: "AI-driven automated code review, PR quality auditing, and regression analysis.",
    loginUrl: "https://app.coderabbit.ai/login",
    dashboardUrl: "https://app.coderabbit.ai",
  },
  {
    id: "figma",
    name: "Figma",
    icon: "Figma",
    category: "Developer & Engineering",
    kind: "token",
    vaultKey: "figma",
    description: "Inspect design files, extract UI tokens, and bridge designs to code.",
    loginUrl: "https://www.figma.com/login",
    tokenUrl: "https://www.figma.com/settings",
    tokenPlaceholder: "figd_... (Figma Access Token)",
    dashboardUrl: "https://www.figma.com/files/recent",
  },

  // ── Messaging & Notifications ──
  {
    id: "slack",
    name: "Slack",
    icon: "Slack",
    category: "Messaging & Notifications",
    kind: "token",
    vaultKey: "slack",
    description: "Send team alerts, monitor deployment channels, and query message threads.",
    loginUrl: "https://slack.com/signin",
    tokenUrl: "https://api.slack.com/apps",
    tokenPlaceholder: "xoxb-... / xoxp-... (Slack Bot Token)",
    dashboardUrl: "https://app.slack.com/client",
  },
  {
    id: "telegram",
    name: "Telegram",
    icon: "Telegram",
    category: "Messaging & Notifications",
    kind: "token",
    vaultKey: "telegram",
    description: "Remote assistant bot control and broadcast channel notifications.",
    tokenUrl: "https://t.me/BotFather",
    tokenPlaceholder: "Telegram bot token",
    dashboardUrl: "https://web.telegram.org",
  },
  {
    id: "whatsapp",
    name: "WhatsApp",
    icon: "WhatsApp",
    category: "Messaging & Notifications",
    kind: "bridge",
    description: "Direct companion messaging and local bridge notifications via linked device.",
    dashboardUrl: "https://web.whatsapp.com",
  },

  // ── Commerce & Everyday Apps ──
  {
    id: "stripe",
    name: "Stripe",
    icon: "Stripe",
    category: "Commerce & Everyday Apps",
    kind: "token",
    vaultKey: "stripe",
    description: "Inspect customer accounts, check balances, and view payment intents.",
    loginUrl: "https://dashboard.stripe.com/login",
    tokenUrl: "https://dashboard.stripe.com/apikeys",
    tokenPlaceholder: "sk_live_... / rk_live_... (Stripe Secret/Restricted Key)",
    dashboardUrl: "https://dashboard.stripe.com",
  },
  {
    id: "spotify",
    name: "Spotify",
    icon: "Spotify",
    category: "Commerce & Everyday Apps",
    kind: "token",
    vaultKey: "spotify",
    description: "Music playback control, playlist switching, and track search.",
    dashboardUrl: "https://open.spotify.com",
  },
  {
    id: "amazon",
    name: "Amazon",
    icon: "Amazon",
    category: "Commerce & Everyday Apps",
    kind: "bridge",
    description: "Local product search bridge for price checks and shopping assistance.",
    dashboardUrl: "https://www.amazon.com",
  },
  {
    id: "swiggy",
    name: "Swiggy",
    icon: "Swiggy",
    category: "Commerce & Everyday Apps",
    kind: "oauth",
    vaultKey: "swiggy",
    description: "Food delivery, Instamart groceries, and dining reservations.",
    dashboardUrl: "https://www.swiggy.com",
  },
];

export interface McpConnectCard {
  server: string;
  state: "unknown" | "down" | "auth_required" | "ready";
  note: string;
  steps: string[];
  qr_image_uri: string | null;
  qr_code_text: string | null;
  pair_url: string | null;
}

interface CodeRabbitStatus {
  state: "not_installed" | "connected" | "disconnected" | "error";
  message: string;
  region: string | null;
  organization: string | null;
}

export function deriveConnectionStates(
  vault: Record<string, string>,
  cards: McpConnectCard[],
  githubConnected: boolean | null,
  coderabbit: CodeRabbitStatus | null,
  available = { vault: true, mcp: true, coderabbit: true },
  telegramChatId = "",
): Record<string, ConnectionState> {
  const byServer = Object.fromEntries(cards.map((card) => [card.server, card]));
  const states: Record<string, ConnectionState> = {};

  for (const provider of MCP_REGISTRY) {
    if (provider.id === "github") {
      states.github =
        githubConnected === true ? "connected" : githubConnected === null ? "error" : "needs_setup";
    } else if (provider.id === "coderabbit") {
      states.coderabbit =
        !available.coderabbit || !coderabbit
          ? "error"
          : coderabbit.state === "connected"
          ? "connected"
          : coderabbit.state === "error"
          ? "error"
          : "needs_setup";
    } else if (provider.id === "swiggy") {
      const parts = ["swiggy-food", "swiggy-instamart", "swiggy-dineout"].map((key) => byServer[key]);
      const ready = parts.filter((part) => part?.state === "ready").length;
      states.swiggy = !available.mcp
        ? "error"
        : ready === 3
        ? "connected"
        : ready > 0
        ? "partial"
        : parts.some((part) => part?.state === "down")
        ? "down"
        : vault.swiggy === "live"
        ? "configured"
        : "needs_setup";
    } else if (provider.id === "whatsapp") {
      const state = byServer.whatsapp?.state;
      states.whatsapp = !available.mcp
        ? "error"
        : state === "ready"
        ? "connected"
        : state === "down"
        ? "down"
        : state === "unknown"
        ? "error"
        : "needs_setup";
    } else if (provider.id === "amazon") {
      states.amazon = !available.mcp
        ? "error"
        : byServer.amazon?.state === "ready"
        ? "available"
        : byServer.amazon?.state === "down"
        ? "down"
        : "needs_setup";
    } else if (provider.id === "spotify") {
      states.spotify = "local";
    } else if (provider.id === "render" || provider.id === "vercel") {
      states[provider.id] = !available.vault
        ? "error"
        : provider.vaultKey && vault[provider.vaultKey] === "live"
        ? "connected"
        : "needs_setup";
    } else if (provider.id === "telegram") {
      states[provider.id] = !available.vault
        ? "error"
        : vault.telegram === "live" && telegramChatId.trim()
        ? "configured"
        : "needs_setup";
    } else {
      states[provider.id] = !available.vault
        ? "error"
        : provider.vaultKey && vault[provider.vaultKey] === "live"
        ? "stored"
        : "unsupported";
    }
  }

  return states;
}

const STATUS_LABEL: Record<ConnectionState, string> = {
  connected: "Connected",
  configured: "Configured",
  stored: "Token stored (inactive)",
  unsupported: "Not supported yet",
  local: "Local controls",
  partial: "Partially connected",
  available: "Bridge reachable",
  needs_setup: "Not connected",
  down: "Unavailable",
  checking: "Checking",
  error: "Status unavailable",
};

const describeError = (error: unknown) => (error instanceof Error ? error.message : String(error));

const CATEGORIES: McpCategory[] = [
  "Cloud & Deployments",
  "Developer & Engineering",
  "Messaging & Notifications",
  "Commerce & Everyday Apps",
];

export function McpServersPanel({
  className = "",
  style = {},
  onClose: _onClose,
}: {
  className?: string;
  style?: React.CSSProperties;
  onClose?: () => void;
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [vault, setVault] = useState<Record<string, string>>({});
  const [cards, setCards] = useState<McpConnectCard[]>([]);
  const [githubConnected, setGithubConnected] = useState<boolean | null>(null);
  const [coderabbit, setCoderabbit] = useState<CodeRabbitStatus | null>(null);
  const [settings, setSettings] = useState({ serverUrl: "", userId: "", telegramChatId: "" });
  const [tokenInputs, setTokenInputs] = useState<Record<string, string>>({});
  const [chatId, setChatId] = useState("");
  const [region, setRegion] = useState<"us" | "eu">("us");
  const [busy, setBusy] = useState<string | null>(null);
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [available, setAvailable] = useState({ vault: true, mcp: true, coderabbit: true });

  const refresh = useCallback(async () => {
    setChecking(true);
    const [settingsResult, vaultResult, cardsResult, rabbitResult] = await Promise.allSettled([
      invoke<{ serverUrl?: string; userId?: string; telegramChatId?: string }>("get_settings"),
      invoke<{ service: string; status: string }[]>("vault_status"),
      invoke<McpConnectCard[]>("mcp_connect_state"),
      invoke<CodeRabbitStatus>("coderabbit_status"),
    ]);

    if (settingsResult.status === "fulfilled") {
      const s = settingsResult.value;
      const next = { serverUrl: s.serverUrl ?? "", userId: s.userId ?? "", telegramChatId: s.telegramChatId ?? "" };
      setSettings(next);
      setChatId(next.telegramChatId);
      if (next.serverUrl) {
        setSidecarBaseUrl(next.serverUrl);
        try {
          const auth = await getOAuthStatus(next.userId || "local-user");
          setGithubConnected(auth.github?.connected === true);
        } catch {
          setGithubConnected(null);
        }
      } else {
        setGithubConnected(null);
      }
    } else {
      setGithubConnected(null);
    }

    setVault(
      vaultResult.status === "fulfilled"
        ? Object.fromEntries(vaultResult.value.map((e) => [e.service, e.status]))
        : {},
    );
    setCards(cardsResult.status === "fulfilled" ? cardsResult.value : []);
    setCoderabbit(rabbitResult.status === "fulfilled" ? rabbitResult.value : null);
    setAvailable({
      vault: vaultResult.status === "fulfilled",
      mcp: cardsResult.status === "fulfilled",
      coderabbit: rabbitResult.status === "fulfilled",
    });

    if (
      rabbitResult.status === "fulfilled" &&
      (rabbitResult.value.region === "us" || rabbitResult.value.region === "eu")
    ) {
      setRegion(rabbitResult.value.region);
    }
    setChecking(false);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen("vault:changed", () => {
      void refresh();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [refresh]);

  const states = useMemo(
    () => deriveConnectionStates(vault, cards, githubConnected, coderabbit, available, settings.telegramChatId),
    [vault, cards, githubConnected, coderabbit, available, settings.telegramChatId],
  );

  const cardFor = (id: string) => cards.find((card) => card.server === id);

  const groupedProviders = useMemo(() => {
    const groups: Record<McpCategory, McpServerConfig[]> = {
      "Cloud & Deployments": [],
      "Developer & Engineering": [],
      "Messaging & Notifications": [],
      "Commerce & Everyday Apps": [],
    };
    for (const provider of MCP_REGISTRY) {
      if (groups[provider.category]) {
        groups[provider.category].push(provider);
      }
    }
    return groups;
  }, []);

  const run = async (id: string, action: () => Promise<unknown>) => {
    setError(null);
    setBusy(id);
    try {
      await action();
      await refresh();
    } catch (e) {
      setError(describeError(e));
    } finally {
      setBusy(null);
    }
  };

  const saveToken = (provider: McpServerConfig) =>
    run(provider.id, async () => {
      const token = tokenInputs[provider.id]?.trim();
      if (!token || !provider.vaultKey) return;
      await invoke("vault_set_token", { service: provider.vaultKey, token, expiresInSecs: 0 });
      setTokenInputs((old) => ({ ...old, [provider.id]: "" }));
    });

  const clearToken = (provider: McpServerConfig) =>
    run(provider.id, async () => {
      if (provider.vaultKey) await invoke("vault_clear_token", { service: provider.vaultKey });
    });

  const saveTelegramChatId = () =>
    run("telegram", async () => {
      await invoke("settings_patch", { patch: { telegramChatId: chatId.trim() } });
    });

  const startOAuth = (provider: "github" | "swiggy") =>
    run(provider, async () => {
      if (!settings.serverUrl) throw new Error("Set up the NEXUS server in Backend settings first.");
      setSidecarBaseUrl(settings.serverUrl);
      await connectOAuth(provider, settings.userId || "local-user");
    });

  const disconnectGithub = () =>
    run("github", async () => {
      if (!settings.serverUrl) throw new Error("NEXUS server is unavailable.");
      setSidecarBaseUrl(settings.serverUrl);
      await disconnectOAuth(settings.userId || "local-user", "github");
      await invoke("github_profile_clear");
    });

  const installCodeRabbit = (autoConnect = true) =>
    run("coderabbit_install", async () => {
      const status = await invoke<CodeRabbitStatus>("coderabbit_install");
      setCoderabbit(status);
      if (autoConnect && status.state !== "connected") {
        setBusy("coderabbit");
        const loginStatus = await invoke<CodeRabbitStatus>("coderabbit_login", { region });
        setCoderabbit(loginStatus);
      }
    });

  const launch = async (url: string) => {
    try {
      await open(url);
    } catch (e) {
      setError(describeError(e));
    }
  };

  return (
    <section className={`mcp-panel ${className}`} style={style} aria-label="MCP">
      <div className="mcp-list">
        {CATEGORIES.map((category) => {
          const providers = groupedProviders[category] || [];
          if (!providers.length) return null;
          const activeInCat = providers.filter(
            (p) => states[p.id] === "connected" || states[p.id] === "configured",
          ).length;

          return (
            <div className="mcp-category-group" key={category}>
              <div className="mcp-category-header">
                <span className="mcp-category-title">{category}</span>
                <span className="mcp-category-badge">
                  {activeInCat > 0 ? `${activeInCat}/${providers.length} active` : `${providers.length} services`}
                </span>
              </div>

              <div className="mcp-category-card">
                {providers.map((provider) => {
                  const expanded = expandedId === provider.id;
                  const isConnecting =
                    busy === provider.id ||
                    (provider.id === "coderabbit" && busy === "coderabbit_install") ||
                    (checking && !cards.length && !Object.keys(vault).length);
                  const state = isConnecting ? "checking" : states[provider.id];
                  const isConnected = state === "connected";
                  const details = cardFor(provider.id);
                  const Icon = McpIcons[provider.icon];

                  return (
                    <div
                      className={`mcp-card-item ${expanded ? "mcp-card-item--expanded" : ""}`}
                      key={provider.id}
                    >
                      <button
                        type="button"
                        className="mcp-row-header"
                        aria-label={`${provider.name}, ${isConnecting ? "Connecting" : STATUS_LABEL[state]}`}
                        aria-expanded={expanded}
                        aria-controls={`mcp-detail-${provider.id}`}
                        onClick={() => {
                          setExpandedId(expanded ? null : provider.id);
                          setError(null);
                        }}
                      >
                        <span className="mcp-row-left">
                          <span className="mcp-brand-icon">
                            <Icon aria-hidden="true" />
                          </span>
                          <span
                            className={`mcp-server-name ${
                              isConnected ? "mcp-server-name--connected" : "mcp-server-name--muted"
                            }`}
                          >
                            {provider.name}
                          </span>
                          {provider.kind === "cli" && <span className="mcp-kind-badge">CLI</span>}
                        </span>
                        <span className="mcp-status-badge">
                          {isConnecting ? (
                            <McpIcons.RadialSpinner className="mcp-status-spinner" />
                          ) : isConnected ? (
                            <McpIcons.CheckmarkBadge className="mcp-status-check" />
                          ) : (
                            <McpIcons.Clock className="mcp-status-timer" />
                          )}
                        </span>
                      </button>

                      {expanded && (
                        <div id={`mcp-detail-${provider.id}`} className="mcp-drawer">
                          <div className="mcp-detail-state">
                            {STATUS_LABEL[state]}
                            {details?.note &&
                            (provider.id === "whatsapp" || provider.id === "amazon" || provider.id === "swiggy")
                              ? ` · ${details.note}`
                              : ""}
                          </div>

                          {/* CodeRabbit */}
                          {provider.id === "coderabbit" && (
                            <>
                              <p>
                                {coderabbit?.message ?? "Could not check the CodeRabbit CLI."}
                                {coderabbit?.organization ? ` · ${coderabbit.organization}` : ""}
                              </p>
                              {coderabbit?.state === "not_installed" ? (
                                <div className="mcp-provider-auth">
                                  <p>The official CodeRabbit CLI is required for automated PR reviews and local code checks.</p>
                                  <div className="mcp-controls">
                                    <button
                                      type="button"
                                      className="mcp-btn mcp-btn--primary"
                                      disabled={busy === "coderabbit_install" || busy === "coderabbit"}
                                      onClick={() => void installCodeRabbit()}
                                    >
                                      {busy === "coderabbit_install"
                                        ? "Installing CodeRabbit CLI..."
                                        : busy === "coderabbit"
                                        ? "Connecting..."
                                        : "Install & Connect CodeRabbit"}
                                    </button>
                                  </div>
                                </div>
                              ) : coderabbit?.state === "connected" ? (
                                <div className="mcp-controls">
                                  <button
                                    type="button"
                                    className="mcp-btn mcp-btn--danger"
                                    disabled={busy === provider.id}
                                    onClick={() => void run(provider.id, () => invoke("coderabbit_logout"))}
                                  >
                                    Disconnect
                                  </button>
                                </div>
                              ) : (
                                <div className="mcp-controls">
                                  <button
                                    type="button"
                                    className="mcp-btn mcp-btn--primary"
                                    disabled={busy === provider.id}
                                    onClick={() =>
                                      void run(provider.id, () => invoke("coderabbit_login", { region }))
                                    }
                                  >
                                    {busy === provider.id ? "Connecting..." : "Sign in with CodeRabbit"}
                                  </button>
                                </div>
                              )}
                            </>
                          )}

                          {/* GitHub */}
                          {provider.id === "github" && (
                            <div className="mcp-controls">
                              <button
                                className="mcp-btn mcp-btn--primary"
                                disabled={busy === provider.id}
                                onClick={() =>
                                  void (state === "connected" ? disconnectGithub() : startOAuth("github"))
                                }
                              >
                                {state === "connected" ? "Disconnect GitHub" : "Connect GitHub"}
                              </button>
                            </div>
                          )}

                          {/* Render & Vercel dedicated API Key entry + Login redirects */}
                          {(provider.id === "render" || provider.id === "vercel") && (
                            <div className="mcp-provider-auth">
                              <p>{provider.description}</p>
                              <div className="mcp-action-links">
                                {provider.loginUrl && (
                                  <button
                                    type="button"
                                    className="mcp-btn mcp-btn--secondary"
                                    onClick={() => void launch(provider.loginUrl!)}
                                  >
                                    Log in to {provider.name} ↗
                                  </button>
                                )}
                                {provider.tokenUrl && (
                                  <button
                                    type="button"
                                    className="mcp-btn mcp-btn--secondary"
                                    onClick={() => void launch(provider.tokenUrl!)}
                                  >
                                    Create {provider.name} {provider.id === "render" ? "API Key" : "Token"} ↗
                                  </button>
                                )}
                              </div>
                              <div className="mcp-controls">
                                <input
                                  type="password"
                                  className="mcp-token-input"
                                  value={tokenInputs[provider.id] ?? ""}
                                  placeholder={provider.tokenPlaceholder || `Paste ${provider.name} key`}
                                  aria-label={`${provider.name} API key`}
                                  onChange={(e) =>
                                    setTokenInputs((old) => ({ ...old, [provider.id]: e.target.value }))
                                  }
                                  onKeyDown={(e) => {
                                    if (e.key === "Enter" && tokenInputs[provider.id]?.trim()) {
                                      void saveToken(provider);
                                    }
                                  }}
                                />
                                <button
                                  type="button"
                                  className="mcp-btn mcp-btn--primary"
                                  disabled={busy === provider.id || !tokenInputs[provider.id]?.trim()}
                                  onClick={() => void saveToken(provider)}
                                >
                                  {vault[provider.vaultKey ?? ""] === "live" ? "Update Key" : "Save Key"}
                                </button>
                                {provider.vaultKey && vault[provider.vaultKey] === "live" && (
                                  <button
                                    type="button"
                                    className="mcp-btn mcp-btn--danger"
                                    disabled={busy === provider.id}
                                    onClick={() => void clearToken(provider)}
                                  >
                                    Remove Key
                                  </button>
                                )}
                              </div>
                            </div>
                          )}

                          {/* Swiggy */}
                          {provider.id === "swiggy" && (
                            <>
                              <p>
                                Food, Instamart, and Dineout share one sign-in. Green appears when all three
                                services respond.
                              </p>
                              <div className="mcp-controls">
                                <button
                                  className="mcp-btn mcp-btn--primary"
                                  disabled={busy === provider.id}
                                  onClick={() => void startOAuth("swiggy")}
                                >
                                  Sign in with Swiggy
                                </button>
                                {vault.swiggy === "live" && (
                                  <button
                                    className="mcp-btn mcp-btn--danger"
                                    disabled={busy === provider.id}
                                    onClick={() => void clearToken(provider)}
                                  >
                                    Clear local token
                                  </button>
                                )}
                              </div>
                            </>
                          )}

                          {/* Telegram */}
                          {provider.id === "telegram" && (
                            <>
                              <p>
                                Use a bot token from BotFather and your owner chat ID. The bot responds only to
                                that chat.
                              </p>
                              <div className="mcp-controls">
                                <input
                                  type="password"
                                  className="mcp-token-input"
                                  value={tokenInputs.telegram ?? ""}
                                  placeholder="Telegram bot token"
                                  aria-label="Telegram bot token"
                                  onChange={(e) =>
                                    setTokenInputs((old) => ({ ...old, telegram: e.target.value }))
                                  }
                                />
                                <button
                                  className="mcp-btn mcp-btn--primary"
                                  disabled={busy === provider.id || !tokenInputs.telegram?.trim()}
                                  onClick={() => void saveToken(provider)}
                                >
                                  Save token
                                </button>
                                {vault.telegram === "live" && (
                                  <button
                                    className="mcp-btn mcp-btn--danger"
                                    disabled={busy === provider.id}
                                    onClick={() => void clearToken(provider)}
                                  >
                                    Remove token
                                  </button>
                                )}
                              </div>
                              <div className="mcp-controls">
                                <input
                                  className="mcp-token-input"
                                  value={chatId}
                                  placeholder="Owner chat ID"
                                  aria-label="Telegram owner chat ID"
                                  onChange={(e) => setChatId(e.target.value)}
                                />
                                <button
                                  className="mcp-btn"
                                  disabled={
                                    busy === provider.id || chatId.trim() === settings.telegramChatId
                                  }
                                  onClick={() => void saveTelegramChatId()}
                                >
                                  Save chat ID
                                </button>
                                <span className="mcp-small-note">
                                  Restart NEXUS after changing the chat ID.
                                </span>
                              </div>
                            </>
                          )}

                          {/* Other Token Providers (Figma, Supabase, Stripe, Slack, Netlify) */}
                          {provider.kind === "token" &&
                            provider.id !== "telegram" &&
                            provider.id !== "render" &&
                            provider.id !== "vercel" && (
                              <div className="mcp-provider-auth">
                                <p>
                                  {provider.id === "spotify"
                                    ? "NEXUS controls the Spotify app through your computer's media controls. Spotify account sign-in is managed in Spotify."
                                    : provider.description ||
                                      "Configure personal access tokens or keys for this provider."}
                                </p>
                                {provider.loginUrl && provider.id !== "spotify" && (
                                  <div className="mcp-action-links">
                                    <button
                                      type="button"
                                      className="mcp-btn mcp-btn--secondary"
                                      onClick={() => void launch(provider.loginUrl!)}
                                    >
                                      Log in to {provider.name} ↗
                                    </button>
                                    {provider.tokenUrl && (
                                      <button
                                        type="button"
                                        className="mcp-btn mcp-btn--secondary"
                                        onClick={() => void launch(provider.tokenUrl!)}
                                      >
                                        Create {provider.name} Token ↗
                                      </button>
                                    )}
                                  </div>
                                )}
                                {provider.vaultKey && provider.id !== "spotify" && (
                                  <div className="mcp-controls">
                                    <input
                                      type="password"
                                      className="mcp-token-input"
                                      value={tokenInputs[provider.id] ?? ""}
                                      placeholder={provider.tokenPlaceholder || "Paste token"}
                                      aria-label={`${provider.name} token`}
                                      onChange={(e) =>
                                        setTokenInputs((old) => ({ ...old, [provider.id]: e.target.value }))
                                      }
                                      onKeyDown={(e) => {
                                        if (e.key === "Enter" && tokenInputs[provider.id]?.trim()) {
                                          void saveToken(provider);
                                        }
                                      }}
                                    />
                                    <button
                                      type="button"
                                      className="mcp-btn mcp-btn--primary"
                                      disabled={busy === provider.id || !tokenInputs[provider.id]?.trim()}
                                      onClick={() => void saveToken(provider)}
                                    >
                                      {vault[provider.vaultKey] === "live" ? "Update Token" : "Save Token"}
                                    </button>
                                    {vault[provider.vaultKey] === "live" && (
                                      <button
                                        className="mcp-btn mcp-btn--danger"
                                        disabled={busy === provider.id}
                                        onClick={() => void clearToken(provider)}
                                      >
                                        Remove Token
                                      </button>
                                    )}
                                  </div>
                                )}
                              </div>
                            )}

                          {/* WhatsApp */}
                          {provider.id === "whatsapp" && (
                            <>
                              <p>
                                {details?.steps?.[0] ??
                                  "Ghost mode is active: speak 'open chat with <contact>' to control WhatsApp directly on your PC."}
                              </p>
                              {details?.steps?.[1] && (
                                <p style={{ marginTop: 4, color: "var(--text-muted, #888)", fontSize: "0.85em" }}>
                                  {details.steps[1]}
                                </p>
                              )}
                              <p style={{ marginTop: 6, fontSize: "0.82em", color: "var(--accent, #60a5fa)" }}>
                                🔒 <strong>Privacy Guarantee:</strong> NEXUS never marks chats as read (no blue ticks) during background checks until you open them personally.
                              </p>
                              {details?.qr_image_uri && (
                                <img
                                  className="mcp-qr"
                                  src={details.qr_image_uri}
                                  alt="WhatsApp pairing QR code"
                                />
                              )}
                              {details?.qr_code_text && (
                                <code className="mcp-qr-code">{details.qr_code_text}</code>
                              )}
                              {details?.pair_url && state !== "down" && (
                                <button className="mcp-btn" onClick={() => void launch(details.pair_url!)}>
                                  Open pairing page ↗
                                </button>
                              )}
                            </>
                          )}

                          {/* Amazon */}
                          {provider.id === "amazon" && (
                            <p>
                              Start the local Amazon product-search bridge on this PC. Reachability confirms
                              the bridge is running; account sign-in is not verified here.
                            </p>
                          )}

                          {/* Dashboard / Token external links */}
                          {provider.tokenUrl && provider.id === "telegram" && (
                            <button className="mcp-btn" onClick={() => void launch(provider.tokenUrl!)}>
                              Open BotFather ↗
                            </button>
                          )}
                          {provider.dashboardUrl &&
                            provider.kind !== "bridge" &&
                            provider.id !== "render" &&
                            provider.id !== "vercel" &&
                            provider.id !== "coderabbit" && (
                              <button
                                className="mcp-btn mcp-btn--quiet"
                                onClick={() => void launch(provider.dashboardUrl!)}
                              >
                                Open {provider.name} ↗
                              </button>
                            )}

                          {error && (
                            <div role="alert" className="mcp-error">
                              {error}
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>

      {/* ── Footer ── */}
      <footer className="mcp-footer">
        <McpIcons.InfoShield className="mcp-footer-icon" aria-hidden="true" />
        <span>All connections are encrypted</span>
      </footer>
    </section>
  );
}
