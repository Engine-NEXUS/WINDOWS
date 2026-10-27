/**
 * Browser Dev Mode Mocks for Tauri IPC commands.
 * Allows NEXUS Command Hub and Settings to run seamlessly in standard browser tabs
 * with realistic live state for accounts, API keys, MCP, insights, and device settings.
 */

export function initBrowserDevMocks() {
  if (typeof window === "undefined" || typeof (window as any).__TAURI_INTERNALS__ !== "undefined") {
    return;
  }

  (window as any).__TAURI_DEV_STANDALONE__ = true;
  const mockStorageKey = "nexus_settings_mock";
  const mockKeysStorageKey = "nexus_api_keys_mock";

  const defaultMockSettings = {
    autostart: true,
    hotkey: "Ctrl+Space",
    autoHideDelay: 8,
    wakeWordEnabled: true,
    wakePhrase: "NEXUS",
    wakeSensitivity: "medium",
    speakerVerification: false,
    meetingModeAuto: true,
    suppressTtsInMeetings: true,
    localSttOnly: false,
    serverUrl: "https://nexus-worker.chitkullakshya.workers.dev",
    userId: "chitkullakshya",
    deviceId: "CHITKUL-ZENBOOK",
    ttsVoice: "en-GB-RyanNeural",
    speechRate: 1.15,
    ttsVolume: 75,
    ttsProvider: "edge",
    edgeTtsVoice: "en-GB-RyanNeural",
    selectedVoice: "jarvis",
    offlineVoiceModel: "bm_george",
    orbPosition: "top",
    orbCapsuleStyle: "classic",
    orbColor: "#f2b859",
    themeMode: "dark",
    visionProvider: "auto",
    visionRace: "sequential",
    activeAiProvider: "gemini",
    activeAiModel: "gemini-2.0-flash",
  };

  const defaultApiKeys = [
    { service: "groq", label: "Groq", has_key: true, masked: "gsk_••••••••••••3aF9", model: "llama-3.3-70b-versatile" },
    { service: "gemini", label: "Google Gemini", has_key: true, masked: "AIza••••••••••••8bX2", model: "gemini-2.0-flash" },
    { service: "openai", label: "OpenAI", has_key: false, masked: "", model: "gpt-4o" },
    { service: "anthropic", label: "Anthropic Claude", has_key: false, masked: "", model: "claude-3-7-sonnet" },
    { service: "deepseek", label: "DeepSeek", has_key: false, masked: "", model: "deepseek-chat" },
    { service: "xai", label: "xAI (Grok)", has_key: false, masked: "", model: "grok-2" },
    { service: "mistral", label: "Mistral AI", has_key: false, masked: "", model: "mistral-large-latest" },
    { service: "openrouter", label: "OpenRouter", has_key: false, masked: "", model: "anthropic/claude-3.5-sonnet" },
    { service: "cerebras", label: "Cerebras", has_key: false, masked: "", model: "llama-3.1-70b" },
    { service: "ollama", label: "Ollama (Local LLM)", has_key: false, masked: "", model: "llama3:latest", baseUrl: "http://localhost:11434/v1" },
  ];

  const listeners = new Map<string, Set<(event: any) => void>>();
  let activityEnabled = true;
  let activityCleared = false;

  const activityStatus = () => {
    const today = new Date();
    const day = (offset: number) => {
      const date = new Date(today);
      date.setDate(date.getDate() + offset);
      return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
    };
    const samples = [
      { day: day(0), apps: [{ app: "Discord", seconds: 11100 }, { app: "Spotify", seconds: 9060 }, { app: "Instagram", seconds: 6720 }, { app: "Chrome", seconds: 3600 }] },
      { day: day(-1), apps: [{ app: "VS Code", seconds: 17400 }, { app: "Chrome", seconds: 8400 }, { app: "Slack", seconds: 3240 }] },
    ];
    return {
      supported: true, can_clear: true, enabled: activityEnabled,
      days: activityCleared ? [] : samples.map(({ day, apps }) => ({
        day, active_seconds: apps.reduce((sum, app) => sum + app.seconds, 0),
        active_minutes: Math.floor(apps.reduce((sum, app) => sum + app.seconds, 0) / 60),
        apps: apps.map((app) => ({ ...app, minutes: Math.floor(app.seconds / 60) })),
      })),
      sharing_available: false,
    };
  };

  (window as any).__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args: any = {}) => {
      // console.log(`[Dev IPC Mock] ${cmd}`, args);

      // Settings storage
      if (cmd === "get_settings") {
        const saved = localStorage.getItem(mockStorageKey);
        return saved ? { ...defaultMockSettings, ...JSON.parse(saved) } : defaultMockSettings;
      }
      if (cmd === "settings_patch") {
        const saved = localStorage.getItem(mockStorageKey);
        const current = saved ? JSON.parse(saved) : defaultMockSettings;
        const updated = { ...current, ...(args.patch || {}) };
        localStorage.setItem(mockStorageKey, JSON.stringify(updated));
        return updated;
      }
      if (cmd === "save_settings") {
        localStorage.setItem(mockStorageKey, JSON.stringify(args.settings || {}));
        return null;
      }
      if (cmd === "get_pending_settings_backdrop" || cmd === "get_pending_sidebar_view") {
        return null;
      }

      // Accounts & Auth
      if (cmd === "google_credentials_status") {
        return { configured: true, has_client_id: true, has_secret: true };
      }
      if (cmd === "google_get_accounts" || cmd === "google_list_accounts") {
        return [
          {
            id: "chitkullakshya@gmail.com",
            email: "chitkullakshya@gmail.com",
            name: "Chitkul Lakshya",
            picture: "",
            primary: true,
            is_primary: true,
            provider: "google",
            handle: "chitkullakshya@gmail.com",
            added_at_ms: Date.now() - 86400000 * 7,
            scopes: ["https://www.googleapis.com/auth/gmail.readonly", "https://www.googleapis.com/auth/calendar.events"],
          },
        ];
      }
      if (cmd === "get_oauth_status") {
        return {
          github: { connected: true, username: "chitkullakshya" },
          google: { connected: true },
        };
      }
      if (cmd === "github_profile") {
        return {
          login: "chitkullakshya",
          name: "Chitkul Lakshya",
          avatar_url: "https://avatars.githubusercontent.com/u/9919?v=4",
          html_url: "https://github.com/chitkullakshya",
        };
      }

      // Insights & Telemetry
      if (cmd === "device_activity_status") return activityStatus();
      if (cmd === "device_activity_set_enabled") {
        activityEnabled = Boolean(args.enabled);
        return activityStatus();
      }
      if (cmd === "device_activity_clear") {
        activityCleared = true;
        return activityStatus();
      }
      if (cmd === "api_keys_count") {
        const saved = localStorage.getItem(mockKeysStorageKey);
        const list = saved ? JSON.parse(saved) : defaultApiKeys;
        return list.filter((k: any) => k.has_key).length;
      }
      if (cmd === "get_usage_stats") {
        return {
          today: 48,
          week: 312,
          topSubsystem: "Voice & Vision",
        };
      }
      if (cmd === "mcp_connect_state") {
        return [
          { server: "github", name: "GitHub MCP", ready: true, toolCount: 14, latencyMs: 12 },
          { server: "filesystem", name: "Local Filesystem", ready: true, toolCount: 8, latencyMs: 2 },
          { server: "brave-search", name: "Brave Search", ready: true, toolCount: 2, latencyMs: 45 },
          { server: "postgres", name: "PostgreSQL DB", ready: true, toolCount: 6, latencyMs: 18 },
        ];
      }
      if (cmd === "vault_status") {
        return [
          { service: "github", status: "stored" },
          { service: "postgres", status: "stored" },
          { service: "brave-search", status: "stored" },
          { service: "vercel", status: "available" },
        ];
      }
      if (cmd === "coderabbit_status") {
        return { connected: true, region: "us", username: "chitkullakshya" };
      }

      // API Keys (Refactored: Groq + Gemini core, AI Model Providers)
      if (cmd === "api_keys_list") {
        const saved = localStorage.getItem(mockKeysStorageKey);
        return saved ? JSON.parse(saved) : defaultApiKeys;
      }
      if (cmd === "api_key_set") {
        const saved = localStorage.getItem(mockKeysStorageKey);
        const list = saved ? JSON.parse(saved) : [...defaultApiKeys];
        const idx = list.findIndex((k: any) => k.service === args.service);
        const maskTail = args.key.length >= 8 ? `••••${args.key.slice(-4)}` : "••••••••";
        if (idx >= 0) {
          list[idx].has_key = true;
          list[idx].masked = maskTail;
        } else {
          list.push({ service: args.service, label: args.service, has_key: true, masked: maskTail });
        }
        localStorage.setItem(mockKeysStorageKey, JSON.stringify(list));
        return { service: args.service, has_key: true, masked: maskTail };
      }
      if (cmd === "api_key_delete") {
        const saved = localStorage.getItem(mockKeysStorageKey);
        const list = saved ? JSON.parse(saved) : [...defaultApiKeys];
        const idx = list.findIndex((k: any) => k.service === args.service);
        if (idx >= 0) {
          list[idx].has_key = false;
          list[idx].masked = "";
        }
        localStorage.setItem(mockKeysStorageKey, JSON.stringify(list));
        return null;
      }

      // Memory Core (Full MemoryStatus contract)
      if (cmd === "memcore_status") {
        return {
          enabled: true,
          encrypted: true,
          facts: 14,
          episodes: 28,
          egress_7d: 19,
          redact_names: false,
          activity: true,
          resume_points: 4,
          briefing: true,
          profile: true,
          routines: true,
          review: false,
          pending: 0,
          curator_note: "Memory core synced with local vault",
          timetable: true,
          slots: 2,
          mail: true,
          google_connected: true,
          mail_items: 3,
          google_needs_signin: false,
          whatsapp: false,
        };
      }
      if (cmd === "memcore_list") {
        return [
          { id: 1, tier: "fact", key: "user_role", value: "Lead Architect & Developer", source: "user_said", trust: "user_said", created: Date.now() - 86400000 * 5, last_seen: Date.now() - 3600000, pinned: true, uses: 24 },
          { id: 2, tier: "fact", key: "preferred_voice", value: "Jarvis / British Ryan (Edge Neural)", source: "user_said", trust: "user_said", created: Date.now() - 86400000 * 4, last_seen: Date.now() - 7200000, pinned: false, uses: 12 },
          { id: 3, tier: "fact", key: "primary_editor", value: "VS Code with NEXUS Extension", source: "derived", trust: "derived", created: Date.now() - 86400000 * 2, last_seen: Date.now() - 1800000, pinned: false, uses: 9 },
        ];
      }
      if (cmd === "memcore_timetable") {
        return [
          { id: "t1", title: "Morning Standup & Code Review", days: [0, 1, 2, 3, 4], start_min: 570, end_min: 600 },
          { id: "t2", title: "Evening Wrap-up & PR Sync", days: [0, 2, 4], start_min: 1020, end_min: 1050 },
        ];
      }
      if (cmd === "memcore_egress_log") {
        return [
          { ts: Date.now() - 3600000, route: "groq/llama-3.3-70b", request: "Check active PR status in repository", memory: "Active project: ULTRON" },
          { ts: Date.now() - 7200000, route: "gemini-2.0-flash", request: "Screen visual grounding inspection", memory: "Window position: top" },
        ];
      }
      if (cmd === "memcore_people") {
        return [
          { jid: "p1", name: "Alex Chen", score: 85, why: "Frequent reviewer on GitHub PRs", vip: true, muted: false },
          { jid: "p2", name: "Sarah Miller", score: 60, why: "Sprint planning contributor", vip: false, muted: false },
        ];
      }
      if (cmd === "memcore_mail_list") {
        return [
          {
            key: "m1",
            sender: "GitHub Notifications",
            email: "notifications@github.com",
            subject: "PR #42 approved and merged into main",
            snippet: "Your pull request #42 was successfully merged after passing CI checks.",
            category: "github",
            urgency: "high",
            reason: "Active repository PR status",
            ts_ms: Date.now() - 14400000,
            account: "chitkullakshya@gmail.com",
          },
        ];
      }
      if (cmd === "memcore_corrections") {
        return [
          { heard: "deploy to production", meant: "deploy to preview", count: 2, last_seen: Date.now() - 86400000, promoted: true },
        ];
      }
      if (cmd === "memcore_routines") {
        return {
          routines: [
            { name: "Morning Briefing", steps: ["check weather", "check github prs", "summarize calendar"], created: Date.now() - 86400000 * 5, last_run: Date.now() - 3600000, runs: 18 },
          ],
          suggestion: null,
          limit: 30,
        };
      }
      if (cmd === "memcore_pending") {
        return [];
      }
      if (cmd === "memory_forget") {
        return true;
      }

      // Actions & Permissions (Full PolicyView contract)
      if (cmd === "actions_get_policy") {
        return {
          policy: {
            version: 1,
            enabled: true,
            mode: "ask",
            use_default_roots: true,
            extra_roots: ["C:\\PROJECTS\\ULTRON", "C:\\Users\\Chitkul Lakshya\\Desktop"],
            protected_paths: ["C:\\Windows", "C:\\Program Files"],
            rules: [
              { effect: "allow", tool: "read", pattern: "*.ts, *.tsx, *.rs" },
              { effect: "ask", tool: "delete", pattern: "*" },
            ],
            max_items: 50,
            max_folder_files: 200,
            quarantine_days: 14,
            audit_days: 30,
            agent_approval: "edit",
          },
          roots: ["C:\\PROJECTS\\ULTRON", "C:\\Users\\Chitkul Lakshya\\Desktop"],
        };
      }
      if (cmd === "actions_trash_list") {
        return [];
      }
      if (cmd === "actions_audit") {
        return [
          { ts: Date.now() - 7200000, phrase: "git status", tool: "exec", risk: "low", origin: "voice", decision: "allow", outcome: "success", detail: "Clean work tree" },
        ];
      }
      if (cmd === "actions_set_policy") {
        return null;
      }

      // Worker API (Device Links & Remote pairing)
      if (cmd === "worker_request") {
        const path = args.path || "";
        if (path === "/v1/links/phone") {
          return {
            status: 200,
            body: { binding: { phone: "+1 (555) 019-2834", verified_at: Date.now() - 86400000 * 30 } },
            message: null,
          };
        }
        if (path === "/v1/links/pending") {
          return { status: 200, body: { requests: [] }, message: null };
        }
        if (path === "/v1/links/list") {
          return {
            status: 200,
            body: {
              links: [
                { link_id: "lnk_1", device_a: "dev_zenbook", device_b: "dev_macbook", device_name: "MacBook Pro M3", created_at: Date.now() - 86400000 * 5 },
              ],
            },
            message: null,
          };
        }
        return { status: 200, body: {}, message: null };
      }

      // Vision
      if (cmd === "vision_key_status") {
        return { gemini_present: true };
      }
      if (cmd === "vision_quota_status") {
        return {
          date: new Date().toISOString().split("T")[0],
          groq: { used: 18, limit: 1000 },
          gemini: { used: 42, limit: 500 },
        };
      }
      if (cmd === "vision_test_key") {
        return { ok: true, detail: "valid" };
      }

      // Identity & Health
      if (cmd === "get_identity_status") {
        return { state: "approved", profileId: "nexus-apex", deviceName: "CHITKUL-ZENBOOK" };
      }
      if (cmd === "get_health_status") {
        return {
          memoryMb: 182,
          uptimeSec: 5420,
          sttPort: true,
          nluPort: true,
          workerReachable: true,
          groqKey: true,
          geminiKey: true,
        };
      }

      // Voice Personas & Audio
      if (cmd === "list_voice_personas") {
        return [
          {
            key: "jarvis",
            name: "Jarvis",
            persona: "Formal British butler with razor-sharp wit and absolute composure",
            accentTag: "British • Formal",
            avatar: "🎩",
            cloudId: "en-GB-RyanNeural",
            kokoroVoice: "bm_george",
            previewPhrase: "At your service, sir. All core subsystems and telemetry rings are nominal.",
          },
          {
            key: "ava",
            name: "Ava",
            persona: "Warm, empathetic and sharp neural intelligence",
            accentTag: "US • Natural Neural",
            avatar: "✨",
            cloudId: "en-US-AvaNeural",
            kokoroVoice: "af_heart",
            previewPhrase: "Hey Lakshya, I'm ready to review your latest PR or inspect the workspace.",
          },
          {
            key: "george",
            name: "Professor George",
            persona: "Measured academic with meticulous technical depth",
            accentTag: "British • Academic",
            avatar: "📚",
            cloudId: "en-GB-ThomasNeural",
            kokoroVoice: "bm_george",
            previewPhrase: "Examining the architectural patterns in the codebase now.",
          },
          {
            key: "sonia",
            name: "Sonia",
            persona: "Crisp, energetic European technical co-pilot",
            accentTag: "UK • Energetic",
            avatar: "⚡",
            cloudId: "en-GB-SoniaNeural",
            kokoroVoice: "bf_alice",
            previewPhrase: "Standing by. Let's push this branch and run our automated test suite.",
          },
        ];
      }
      if (cmd === "get_voice_status") {
        return {
          voice_key: "jarvis",
          cloud_id: "en-GB-RyanNeural",
          twin: "bm_george",
          sync_state: "ready",
        };
      }
      if (cmd === "get_voice_transport") {
        return { transport: "cloud" };
      }
      if (cmd === "set_voice_preference") {
        return {
          voiceKey: args.voiceKey || "jarvis",
          cloudId: args.voiceKey === "ava" ? "en-US-AvaNeural" : "en-GB-RyanNeural",
          syncState: "ready",
        };
      }

      // Hand Tracking & Agent Watch
      if (cmd === "hand_status") {
        return { active: false };
      }
      if (cmd === "hand_start" || cmd === "hand_stop") {
        return null;
      }
      if (cmd === "agent_watch_unavailable_reason") {
        return null;
      }
      if (cmd === "agent_watch_list_adapters") {
        return ["claude-code", "codestral", "cursor"];
      }
      if (cmd === "agent_watch_list_watches") {
        return { watches: [] };
      }
      if (cmd === "agent_watch_stop") {
        return null;
      }

      // Vault & Tokens
      if (cmd === "vault_set_token" || cmd === "vault_clear_token") {
        return null;
      }
      if (cmd === "github_profile_clear") {
        return null;
      }

      // Window & UI actions
      if (cmd === "hide_settings_sidebar" || cmd === "close_settings_window") {
        console.log("[Dev Mode] Window close requested");
        return null;
      }
      if (cmd === "set_sidebar_dock") {
        console.log(`[Dev Mode] Dock set to: ${args.dock}`);
        return null;
      }
      if (cmd === "open_orb_studio_window") {
        console.log("[Dev Mode] Orb Studio opened");
        return null;
      }
      if (cmd === "set_orb_position" || cmd === "set_orb_capsule_style" || cmd === "preview_voice" || cmd === "preview_wake_sound" || cmd === "test_wake_orb") {
        return null;
      }

      // Events
      if (cmd === "plugin:event|listen") {
        const eventName = args.event;
        const handlerId = Math.floor(Math.random() * 1000000);
        if (!listeners.has(eventName)) {
          listeners.set(eventName, new Set());
        }
        return handlerId;
      }
      if (cmd === "plugin:event|unlisten") {
        return null;
      }

      return null;
    },
    transformCallback: (cb: any) => {
      return typeof cb === "function" ? 1 : 0;
    },
    unregisterCallback: () => {},
    metadata: {
      currentWindow: { label: "settings" },
    },
  };
}
