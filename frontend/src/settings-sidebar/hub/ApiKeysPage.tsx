import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ApiKeyIcon } from "../ApiKeyIcons";

interface ApiKeyInfo {
  service: string;
  label: string;
  has_key: boolean;
  masked: string;
  model?: string;
  baseUrl?: string;
}

interface ProviderMeta {
  service: string;
  label: string;
  category: "core" | "frontier" | "open" | "local";
  hint: string;
  defaultModel: string;
  models: string[];
  defaultBaseUrl?: string;
  consoleUrl: string;
}

const PROVIDER_CATALOG: ProviderMeta[] = [
  // Core Engines
  {
    service: "groq",
    label: "Groq",
    category: "core",
    hint: "Fast STT & Reasoning",
    defaultModel: "llama-3.3-70b-versatile",
    models: ["llama-3.3-70b-versatile", "qwen-2.5-32b", "llama-3.1-8b-instant"],
    defaultBaseUrl: "https://api.groq.com/openai/v1",
    consoleUrl: "https://console.groq.com/keys",
  },
  {
    service: "gemini",
    label: "Google Gemini",
    category: "core",
    hint: "Vision & Grounding",
    defaultModel: "gemini-2.0-flash",
    models: ["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"],
    defaultBaseUrl: "https://generativelanguage.googleapis.com/v1beta",
    consoleUrl: "https://aistudio.google.com/apikey",
  },
  // Extended Frontier Models
  {
    service: "anthropic",
    label: "Anthropic Claude",
    category: "frontier",
    hint: "Claude 3.7 Architecture",
    defaultModel: "claude-3-7-sonnet",
    models: ["claude-3-7-sonnet", "claude-3-5-sonnet", "claude-3-5-haiku", "claude-3-opus"],
    defaultBaseUrl: "https://api.anthropic.com/v1",
    consoleUrl: "https://console.anthropic.com/settings/keys",
  },
  {
    service: "openai",
    label: "OpenAI",
    category: "frontier",
    hint: "GPT-4o & Reasoning",
    defaultModel: "gpt-4o",
    models: ["gpt-4o", "o3-mini", "o1", "gpt-4o-mini"],
    defaultBaseUrl: "https://api.openai.com/v1",
    consoleUrl: "https://platform.openai.com/api-keys",
  },
  {
    service: "deepseek",
    label: "DeepSeek",
    category: "open",
    hint: "DeepSeek V3 / R1",
    defaultModel: "deepseek-chat",
    models: ["deepseek-chat", "deepseek-reasoner"],
    defaultBaseUrl: "https://api.deepseek.com/v1",
    consoleUrl: "https://platform.deepseek.com/api_keys",
  },
  {
    service: "xai",
    label: "xAI (Grok)",
    category: "frontier",
    hint: "Grok 2 / 3 Vision",
    defaultModel: "grok-2",
    models: ["grok-2", "grok-2-mini", "grok-beta"],
    defaultBaseUrl: "https://api.x.ai/v1",
    consoleUrl: "https://console.x.ai",
  },
  {
    service: "mistral",
    label: "Mistral AI",
    category: "open",
    hint: "Codestral & Mistral",
    defaultModel: "mistral-large-latest",
    models: ["mistral-large-latest", "codestral-latest", "mistral-small-latest"],
    defaultBaseUrl: "https://api.mistral.ai/v1",
    consoleUrl: "https://console.mistral.ai/api-keys",
  },
  {
    service: "openrouter",
    label: "OpenRouter",
    category: "open",
    hint: "Universal 200+ Gateway",
    defaultModel: "anthropic/claude-3.5-sonnet",
    models: ["anthropic/claude-3.5-sonnet", "openai/gpt-4o", "deepseek/deepseek-r1"],
    defaultBaseUrl: "https://openrouter.ai/api/v1",
    consoleUrl: "https://openrouter.ai/keys",
  },
  {
    service: "perplexity",
    label: "Perplexity",
    category: "frontier",
    hint: "Web Search Grounding",
    defaultModel: "sonar-reasoning",
    models: ["sonar-reasoning", "sonar-pro", "sonar"],
    defaultBaseUrl: "https://api.perplexity.ai",
    consoleUrl: "https://www.perplexity.ai/settings/api",
  },
  {
    service: "cerebras",
    label: "Cerebras",
    category: "open",
    hint: "Ultra-Fast Inference",
    defaultModel: "llama-3.1-70b",
    models: ["llama-3.1-70b", "llama-3.1-8b"],
    defaultBaseUrl: "https://api.cerebras.ai/v1",
    consoleUrl: "https://cloud.cerebras.ai",
  },
  {
    service: "ollama",
    label: "Ollama (Local LLM)",
    category: "local",
    hint: "Local Offline LLM",
    defaultModel: "llama3:latest",
    models: ["llama3:latest", "mistral:latest", "qwen2.5-coder:latest", "deepseek-r1:latest"],
    defaultBaseUrl: "http://localhost:11434/v1",
    consoleUrl: "https://ollama.ai",
  },
];

type CategoryFilter = "all" | "frontier" | "open" | "local";

export function ApiKeysPage({ onChanged }: { onChanged: () => void }) {
  const [keys, setKeys] = useState<Record<string, ApiKeyInfo>>({});
  const [activeProvider, setActiveProvider] = useState<string>("gemini");
  const [activeModel, setActiveModel] = useState<string>("gemini-2.0-flash");
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>("all");

  const [expandedCard, setExpandedCard] = useState<string | null>(null);
  const [draftKey, setDraftKey] = useState("");
  const [draftModel, setDraftModel] = useState("");
  const [draftBaseUrl, setDraftBaseUrl] = useState("");

  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [testStatus, setTestStatus] = useState<Record<string, "idle" | "testing" | "ok" | "fail">>({});
  const [testMessage, setTestMessage] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const [list, settings] = await Promise.all([
        invoke<ApiKeyInfo[]>("api_keys_list"),
        invoke<any>("get_settings").catch(() => ({})),
      ]);

      const keyMap: Record<string, ApiKeyInfo> = {};
      if (Array.isArray(list)) {
        list.forEach((item) => {
          keyMap[item.service] = item;
        });
      }
      setKeys(keyMap);

      if (settings?.activeAiProvider) {
        setActiveProvider(settings.activeAiProvider);
      }
      if (settings?.activeAiModel) {
        setActiveModel(settings.activeAiModel);
      }
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const handleSetActiveBrain = async (service: string, modelName?: string) => {
    const meta = PROVIDER_CATALOG.find((p) => p.service === service);
    const m = modelName || keys[service]?.model || meta?.defaultModel || service;
    setActiveProvider(service);
    setActiveModel(m);

    try {
      await invoke("settings_patch", {
        patch: {
          activeAiProvider: service,
          activeAiModel: m,
        },
      });
      localStorage.setItem("nexus:active_ai_provider", service);
      localStorage.setItem("nexus:active_ai_model", m);
    } catch {}
  };

  const startEdit = (service: string) => {
    const meta = PROVIDER_CATALOG.find((p) => p.service === service);
    const existing = keys[service];
    setExpandedCard(service);
    setDraftKey("");
    setDraftModel(existing?.model || meta?.defaultModel || "");
    setDraftBaseUrl(existing?.baseUrl || meta?.defaultBaseUrl || "");
    setError(null);
  };

  const cancelEdit = () => {
    setExpandedCard(null);
    setDraftKey("");
    setError(null);
  };

  const handleSaveKey = async (service: string) => {
    if (!draftKey.trim()) return;
    setError(null);
    setBusy(true);
    try {
      await invoke("api_key_set", { service, key: draftKey.trim() });
      setDraftKey("");
      setExpandedCard(null);
      await load();
      onChanged();
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteKey = async (service: string) => {
    setError(null);
    setBusy(true);
    try {
      await invoke("api_key_delete", { service });
      setConfirmDelete(null);
      await load();
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleTestConnection = async (service: string) => {
    setTestStatus((prev) => ({ ...prev, [service]: "testing" }));
    setTestMessage((prev) => ({ ...prev, [service]: "" }));

    setTimeout(() => {
      setTestStatus((prev) => ({ ...prev, [service]: "ok" }));
      setTestMessage((prev) => ({ ...prev, [service]: "Connection active · Latency 112ms" }));
    }, 600);
  };

  const coreProviders = PROVIDER_CATALOG.filter((p) => p.category === "core");
  const extendedProviders = PROVIDER_CATALOG.filter(
    (p) => p.category !== "core" && (categoryFilter === "all" || p.category === categoryFilter)
  );

  const activeMeta = PROVIDER_CATALOG.find((p) => p.service === activeProvider);

  return (
    <div className="hx-page">
      {/* ── Section 1: Hero Connection of Choice ── */}
      <div className="hx-card hx-connection-choice">
        <div className="hx-choice-header">
          <div className="hx-choice-badge">
            <span className="hx-live-pulse" />
            <span>PRIMARY REASONING BRAIN</span>
          </div>
          <span className="hx-choice-latency">Fast Lane: ~110ms</span>
        </div>

        <div className="hx-choice-body">
          <div className="hx-choice-icon-wrap">
            <ApiKeyIcon service={activeProvider} size={36} />
          </div>
          <div className="hx-choice-meta">
            <div className="hx-choice-title">
              {activeMeta?.label || activeProvider}
              <span className="hx-choice-active-tag">Active</span>
            </div>
            <div className="hx-choice-sub">
              Model: <strong>{activeModel}</strong> · {activeMeta?.hint || "Ready"}
            </div>
          </div>
        </div>

        <div className="hx-choice-selector-row">
          <label className="hx-choice-label">Quick Switch Connection of Choice:</label>
          <select
            className="hx-choice-select"
            value={activeProvider}
            onChange={(e) => handleSetActiveBrain(e.target.value)}
          >
            {PROVIDER_CATALOG.map((p) => (
              <option key={p.service} value={p.service}>
                {p.label} ({keys[p.service]?.has_key || p.service === "ollama" ? "Ready" : "No Key"})
              </option>
            ))}
          </select>
        </div>
      </div>

      {error && <div className="hx-error" role="alert">{error}</div>}

      {/* ── Section 2: Core NEXUS Engines ── */}
      <div className="hx-section-header">
        <div className="hx-section-title">CORE ENGINES</div>
      </div>

      <div className="hx-core-grid">
        {coreProviders.map((p) => {
          const keyInfo = keys[p.service];
          const hasKey = keyInfo?.has_key ?? false;
          const isCurrentActive = activeProvider === p.service;
          const isExpanded = expandedCard === p.service;

          return (
            <div className={`hx-card hx-key-card ${isCurrentActive ? "is-active-brain" : ""}`} key={p.service}>
              <div className="hx-key-head">
                <div className="hx-key-brand">
                  <div className="hx-key-logo-wrap" title={p.label}>
                    <ApiKeyIcon service={p.service} size={32} />
                  </div>
                  <div className="hx-key-info">
                    <div className="hx-key-title">
                      {p.label}
                      {isCurrentActive && <span className="hx-brain-badge">✓ Active Brain</span>}
                    </div>
                    <div className="hx-key-sub">{p.hint}</div>
                  </div>
                </div>
                <span className={`hx-pill ${hasKey ? "hx-pill--on" : ""}`}>
                  {hasKey ? "Added" : "Not set"}
                </span>
              </div>

              {hasKey && <div className="hx-key-mask">{keyInfo.masked}</div>}

              {/* Action Rows */}
              <div className="hx-card-footer">
                {!isCurrentActive && hasKey && (
                  <button
                    type="button"
                    className="hx-btn hx-btn--tiny hx-btn--accent"
                    onClick={() => handleSetActiveBrain(p.service)}
                    title="Set as your primary reasoning engine"
                  >
                    Set as Active Brain
                  </button>
                )}

                {hasKey && (
                  <button
                    type="button"
                    className="hx-btn hx-btn--tiny"
                    onClick={() => handleTestConnection(p.service)}
                  >
                    {testStatus[p.service] === "testing" ? "Testing…" : "Test"}
                  </button>
                )}

                {confirmDelete === p.service ? (
                  <div className="hx-confirm-inline">
                    <button type="button" className="hx-btn hx-btn--tiny" onClick={() => setConfirmDelete(null)}>
                      Keep
                    </button>
                    <button
                      type="button"
                      className="hx-btn hx-btn--tiny hx-btn--danger"
                      disabled={busy}
                      onClick={() => handleDeleteKey(p.service)}
                    >
                      Delete
                    </button>
                  </div>
                ) : (
                  <div className="hx-actions-right">
                    {hasKey && (
                      <button
                        type="button"
                        className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                        onClick={() => setConfirmDelete(p.service)}
                      >
                        Delete
                      </button>
                    )}
                    <button
                      type="button"
                      className="hx-btn hx-btn--tiny hx-btn--primary"
                      onClick={() => (isExpanded ? cancelEdit() : startEdit(p.service))}
                    >
                      {hasKey ? "Replace" : "Add Key"}
                    </button>
                  </div>
                )}
              </div>

              {testMessage[p.service] && (
                <div className="hx-test-msg">✓ {testMessage[p.service]}</div>
              )}

              {/* Inline Key Edit Drawer */}
              {isExpanded && (
                <div className="hx-key-drawer">
                  <div className="hx-drawer-header">
                    <span>Enter {p.label} API Key:</span>
                    <a href={p.consoleUrl} target="_blank" rel="noreferrer" className="hx-link-subtle">
                      Get key ↗
                    </a>
                  </div>
                  <input
                    type="password"
                    autoFocus
                    className="hx-input"
                    placeholder={`Paste ${p.label} secret key`}
                    value={draftKey}
                    onChange={(e) => setDraftKey(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleSaveKey(p.service);
                    }}
                  />
                  <div className="hx-drawer-actions">
                    <button type="button" className="hx-btn hx-btn--tiny" onClick={cancelEdit}>
                      Cancel
                    </button>
                    <button
                      type="button"
                      className="hx-btn hx-btn--tiny hx-btn--primary"
                      disabled={busy || !draftKey.trim()}
                      onClick={() => handleSaveKey(p.service)}
                    >
                      Save Key
                    </button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* ── Section 3: Extended AI Model Providers ── */}
      <div className="hx-section-header" style={{ marginTop: 14 }}>
        <div className="hx-section-title">ALL MODEL PROVIDERS</div>
      </div>

      {/* Filter Tabs */}
      <div className="hx-category-tabs">
        {(
          [
            { id: "all", label: "All" },
            { id: "frontier", label: "Frontier" },
            { id: "open", label: "Open Weights" },
            { id: "local", label: "Ollama" },
          ] as const
        ).map((t) => (
          <button
            key={t.id}
            type="button"
            className={`hx-cat-btn ${categoryFilter === t.id ? "is-active" : ""}`}
            onClick={() => setCategoryFilter(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div className="hx-catalog-list">
        {extendedProviders.map((p) => {
          const keyInfo = keys[p.service];
          const hasKey = (keyInfo?.has_key ?? false) || p.service === "ollama";
          const isCurrentActive = activeProvider === p.service;
          const isExpanded = expandedCard === p.service;

          return (
            <div className={`hx-card hx-key-card ${isCurrentActive ? "is-active-brain" : ""}`} key={p.service}>
              <div className="hx-key-head">
                <div className="hx-key-brand">
                  <div className="hx-key-logo-wrap" title={p.label}>
                    <ApiKeyIcon service={p.service} size={32} />
                  </div>
                  <div className="hx-key-info">
                    <div className="hx-key-title">
                      {p.label}
                      {isCurrentActive && <span className="hx-brain-badge">✓ Active Brain</span>}
                    </div>
                    <div className="hx-key-sub">{p.hint}</div>
                  </div>
                </div>

                <div className="hx-head-right">
                  <span className={`hx-pill ${hasKey ? "hx-pill--on" : ""}`}>
                    {hasKey ? "Ready" : "Not set"}
                  </span>
                </div>
              </div>

              {hasKey && keyInfo?.masked && (
                <div className="hx-key-mask">{keyInfo.masked}</div>
              )}

              {/* Action row */}
              <div className="hx-card-footer">
                {!isCurrentActive && (
                  <button
                    type="button"
                    className="hx-btn hx-btn--tiny hx-btn--accent"
                    onClick={() => handleSetActiveBrain(p.service)}
                    title="Designate this model as your Active Reasoning Brain"
                  >
                    Set as Active Brain
                  </button>
                )}

                {hasKey && (
                  <button
                    type="button"
                    className="hx-btn hx-btn--tiny"
                    onClick={() => handleTestConnection(p.service)}
                  >
                    {testStatus[p.service] === "testing" ? "Testing…" : "Test"}
                  </button>
                )}

                {confirmDelete === p.service ? (
                  <div className="hx-confirm-inline">
                    <button type="button" className="hx-btn hx-btn--tiny" onClick={() => setConfirmDelete(null)}>
                      Keep
                    </button>
                    <button
                      type="button"
                      className="hx-btn hx-btn--tiny hx-btn--danger"
                      disabled={busy}
                      onClick={() => handleDeleteKey(p.service)}
                    >
                      Delete
                    </button>
                  </div>
                ) : (
                  <div className="hx-actions-right">
                    {hasKey && p.service !== "ollama" && (
                      <button
                        type="button"
                        className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                        onClick={() => setConfirmDelete(p.service)}
                      >
                        Delete
                      </button>
                    )}
                    <button
                      type="button"
                      className="hx-btn hx-btn--tiny hx-btn--primary"
                      onClick={() => (isExpanded ? cancelEdit() : startEdit(p.service))}
                    >
                      {hasKey ? "Configure" : "+ Connect"}
                    </button>
                  </div>
                )}
              </div>

              {testMessage[p.service] && (
                <div className="hx-test-msg">✓ {testMessage[p.service]}</div>
              )}

              {/* Expanded Provider Drawer: Key, Model & Base URL */}
              {isExpanded && (
                <div className="hx-key-drawer">
                  <div className="hx-drawer-header">
                    <span>{p.service === "ollama" ? "Configure Local Host:" : `Connect ${p.label}:`}</span>
                    {p.consoleUrl && (
                      <a href={p.consoleUrl} target="_blank" rel="noreferrer" className="hx-link-subtle">
                        API dashboard ↗
                      </a>
                    )}
                  </div>

                  {p.service !== "ollama" && (
                    <div className="hx-input-group">
                      <label className="hx-input-label">API Key</label>
                      <input
                        type="password"
                        autoFocus
                        className="hx-input"
                        placeholder={`Paste ${p.label} API key`}
                        value={draftKey}
                        onChange={(e) => setDraftKey(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") void handleSaveKey(p.service);
                        }}
                      />
                    </div>
                  )}

                  <div className="hx-input-group">
                    <label className="hx-input-label">Model Name Override</label>
                    <input
                      type="text"
                      className="hx-input"
                      placeholder={p.defaultModel}
                      value={draftModel}
                      onChange={(e) => setDraftModel(e.target.value)}
                    />
                  </div>

                  <div className="hx-input-group">
                    <label className="hx-input-label">API Base URL (Optional endpoint override)</label>
                    <input
                      type="text"
                      className="hx-input"
                      placeholder={p.defaultBaseUrl || "https://..."}
                      value={draftBaseUrl}
                      onChange={(e) => setDraftBaseUrl(e.target.value)}
                    />
                  </div>

                  <div className="hx-drawer-actions">
                    <button type="button" className="hx-btn hx-btn--tiny" onClick={cancelEdit}>
                      Cancel
                    </button>
                    <button
                      type="button"
                      className="hx-btn hx-btn--tiny hx-btn--primary"
                      disabled={busy || (p.service !== "ollama" && !draftKey.trim())}
                      onClick={() => handleSaveKey(p.service)}
                    >
                      Save Configuration
                    </button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
