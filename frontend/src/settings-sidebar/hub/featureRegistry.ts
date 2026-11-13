/**
 * Every NEXUS feature that has a settings switch, in one list. The Features page renders this list and nothing else, so a
 * new switch is one row here. `scripts/check-feature-registry.mjs` fails CI when a boolean in `NexusSettings` is neither
 * a row here nor in `featureInternal.json`.
 *
 * Phase 1 of the Features page: on/off only. Readiness badges and Test buttons come in later phases, so the fields for
 * them are optional and the page shows them only when present (never an invented value).
 */

export type FeatureGroup =
  | "Voice"
  | "Screen and vision"
  | "Desktop control"
  | "Memory and proactive"
  | "Notifications"
  | "Agents"
  | "System";

export const GROUP_ORDER: FeatureGroup[] = [
  "Voice",
  "Screen and vision",
  "Desktop control",
  "Memory and proactive",
  "Notifications",
  "Agents",
  "System",
];

/** What turning it on exposes: nothing, the microphone or camera, data sent to a cloud service, or actions on the desktop. */
export type Risk = "safe" | "mic" | "cloud" | "desktop";

/** How soon a change takes effect. */
export type Apply = "live" | "restart" | "trigger";

export interface Feature {
  id: string;
  group: FeatureGroup;
  title: string;
  what: string;
  /** Key in settings.json / NexusSettings (camelCase). */
  key: string;
  kind: "bool" | "enum";
  /** For `enum`: the allowed values, first one is the safe/off value. */
  options?: string[];
  /** The value the backend uses when the key is missing. */
  def: boolean | string;
  apply: Apply;
  risk: Risk;
  /** Needs another feature's key to be on to have any effect. */
  dependsOn?: string;
  /** Turning it off weakens a safety property: the page warns first. */
  safety?: boolean;
  /** Safe mode never switches this one off: it protects (Voice Lock, local-only speech, wake check) or is the way back. */
  keepInSafeMode?: boolean;
  /** Readiness level from the scorecard (1 production ready .. 4 planned). Absent until the generator exists. */
  readiness?: 1 | 2 | 3 | 4;
}

type Row = Omit<Feature, "kind" | "apply" | "risk"> & Partial<Pick<Feature, "apply" | "risk">>;

const bool = (r: Row): Feature => ({ kind: "bool", apply: "live", risk: "safe", ...r });

export const FEATURES: Feature[] = [
  // ── Voice ──
  bool({ id: "wake-verify", group: "Voice", title: "Check the wake word with speech recognition", what: "A second check that you really said the wake word, before NEXUS wakes.", key: "verifyWake", def: true, risk: "cloud", safety: true, keepInSafeMode: true }),
  bool({ id: "wake-fast-path", group: "Voice", title: "Fast wake for the owner", what: "Experimental shortcut for waking on the owner's voice.", key: "wakeOwnerFastPath", def: false, risk: "mic" }),
  bool({ id: "wake-sound", group: "Voice", title: "Wake-up sound", what: "A short chime when NEXUS starts listening.", key: "wakeSound", def: true }),
  bool({ id: "mic-keepalive", group: "Voice", title: "Keep the microphone warm", what: "Keeps the microphone ready between turns so the wake word reacts fast.", key: "micKeepAlive", def: true, risk: "mic" }),
  bool({ id: "barge-in", group: "Voice", title: "Interrupt NEXUS without the wake word", what: "Speak over a reply to stop it.", key: "bargeInNoWake", def: true, risk: "mic" }),
  bool({ id: "voice-lock", group: "Voice", title: "Voice Lock (owner only)", what: "Only the enrolled owner's voice can give commands. Needs an enrolled voice profile.", key: "speakerVerification", def: false, risk: "mic", safety: true, keepInSafeMode: true }),
  bool({ id: "directed-gate", group: "Voice", title: "Ignore speech not meant for NEXUS", what: "In Ghost mode, drops TV, side talk and NEXUS's own echo.", key: "directedGate", def: true, safety: true }),
  bool({ id: "stt-local", group: "Voice", title: "Local speech recognition only", what: "Audio never leaves this laptop. Ignores your Groq key.", key: "localSttOnly", def: false, risk: "mic", keepInSafeMode: true }),
  bool({ id: "stt-hints", group: "Voice", title: "Command hints for other voices", what: "Biases recognition toward common commands for unrecognised speakers.", key: "sttCommandHints", def: false }),
  bool({ id: "stt-gate", group: "Voice", title: "Drop low-confidence transcripts", what: "Rejects speech the recogniser itself is unsure about.", key: "sttConfidenceGate", def: false }),
  bool({ id: "stt-log-conf", group: "Voice", title: "Log recognition confidence", what: "Writes a confidence line per turn to the log, for tuning.", key: "sttLogConfidence", def: false }),
  bool({ id: "stt-diag", group: "Voice", title: "Voice diagnostics (24 h)", what: "Keeps short encrypted clips for debugging; they expire after 24 hours.", key: "sttDiagnostics", def: false, risk: "mic" }),

  // ── Screen and vision ──
  bool({ id: "screen-tour", group: "Screen and vision", title: "Narrated screen tour", what: "\"Analyse my screen\" points at things while it explains them. Sends a screenshot to a vision model.", key: "screenTour", def: true, risk: "cloud" }),
  bool({ id: "maps-embed", group: "Screen and vision", title: "Maps panel", what: "Shows directions and places in a panel.", key: "mapsEmbed", def: true }),

  // ── Desktop control ──
  bool({ id: "ghost-ack", group: "Desktop control", title: "Ghost: say how many commands are queued", what: "A short spoken note when several commands are waiting.", key: "ghostDepthAck", def: true }),
  bool({ id: "hand-mode", group: "Desktop control", title: "Hand Mode (camera pointer)", what: "Lets \"hand mode\" use the camera to move the cursor.", key: "handMode", def: true, risk: "desktop" }),
  bool({ id: "hand-preview", group: "Desktop control", title: "Hand Mode: preview only", what: "Shows what it would click without touching the mouse.", key: "handMousePreview", def: false, dependsOn: "handMode" }),
  bool({ id: "hand-camera", group: "Desktop control", title: "Hand Mode: show camera picture", what: "Debug view of the camera frame.", key: "handShowCamera", def: false, risk: "mic", dependsOn: "handMode" }),
  bool({ id: "hand-trace", group: "Desktop control", title: "Hand Mode: record landmark numbers", what: "Writes landmark numbers (no images) to a file, for tuning.", key: "handTrace", def: false, dependsOn: "handMode" }),
  bool({ id: "hand-ghost", group: "Desktop control", title: "Hand Mode starts with Ghost", what: "Entering Ghost also starts Hand Mode.", key: "handWithGhost", def: false, risk: "desktop", dependsOn: "handMode" }),
  bool({ id: "hand-snap", group: "Desktop control", title: "Hand Mode: snap to controls", what: "A click just outside a control moves to the nearest one.", key: "handSnap", def: true, dependsOn: "handMode" }),
  bool({ id: "hand-gestures", group: "Desktop control", title: "Hand Mode: tab and window gestures", what: "Swipes switch tabs and windows.", key: "handGestures", def: true, risk: "desktop", dependsOn: "handMode" }),
  bool({ id: "hand-gestures-desktop", group: "Desktop control", title: "Hand Mode: desktop gestures", what: "Four-finger gestures for virtual desktops. Off until its threshold is checked.", key: "handGesturesDesktop", def: false, risk: "desktop", dependsOn: "handMode" }),
  bool({ id: "hand-focus", group: "Desktop control", title: "Hand Mode: Focus Mode", what: "Pauses background loops while Hand Mode runs.", key: "handFocusMode", def: true, dependsOn: "handMode" }),
  bool({ id: "hand-focus-apps", group: "Desktop control", title: "Hand Mode: lower heavy apps", what: "Lowers the priority of up to six heavy apps while Hand Mode runs.", key: "handFocusLowerApps", def: true, dependsOn: "handFocusMode" }),

  // ── Memory and proactive ──
  bool({ id: "memcore", group: "Memory and proactive", title: "Memory Core", what: "The sealed memory store everything below builds on.", key: "memcore", def: true }),
  bool({ id: "mem-activity", group: "Memory and proactive", title: "Resume points", what: "Remembers where you left off in apps.", key: "memcoreActivity", def: true, dependsOn: "memcore" }),
  bool({ id: "mem-briefing", group: "Memory and proactive", title: "Daily briefing", what: "Once a day: where you left off, mail and calendar.", key: "memcoreBriefing", def: true, dependsOn: "memcore" }),
  bool({ id: "mem-profile", group: "Memory and proactive", title: "About-me block", what: "Pinned facts sent with every cloud request.", key: "memcoreProfile", def: true, risk: "cloud", dependsOn: "memcore" }),
  bool({ id: "mem-seed", group: "Memory and proactive", title: "Seed memory from accounts", what: "Learns basics from connected accounts.", key: "memcoreAccountSeed", def: true, dependsOn: "memcore" }),
  bool({ id: "mem-routines", group: "Memory and proactive", title: "Routine suggestions", what: "Offers to save app sequences you repeat.", key: "memcoreRoutines", def: true, dependsOn: "memcore" }),
  bool({ id: "mem-timetable", group: "Memory and proactive", title: "Timetable reminders", what: "Reminds you of your timetable slots.", key: "memcoreTimetable", def: true, dependsOn: "memcore" }),
  bool({ id: "mem-leaveby", group: "Memory and proactive", title: "Leave-by reminders", what: "\"Time to leave\" from calendar events and traffic.", key: "memcoreLeaveBy", def: true, dependsOn: "memcore" }),
  bool({ id: "mem-session", group: "Memory and proactive", title: "Session restore", what: "Offers to reopen your apps after a restart.", key: "memcoreSession", def: true, dependsOn: "memcore" }),
  bool({ id: "mem-health", group: "Memory and proactive", title: "Health notes", what: "Mentions problems NEXUS notices with itself.", key: "memcoreHealth", def: true, dependsOn: "memcore" }),
  bool({ id: "mem-redact", group: "Memory and proactive", title: "Hide names from the cloud", what: "Swaps known names for Person A, B before cloud requests.", key: "memcoreRedactNames", def: false, dependsOn: "memcore", safety: true }),
  bool({ id: "mem-review", group: "Memory and proactive", title: "Memory suggestions (cloud)", what: "A free model suggests facts to remember; you approve each.", key: "memcoreReview", def: false, risk: "cloud", dependsOn: "memcore" }),

  // ── Notifications ──
  bool({ id: "mail-watch", group: "Notifications", title: "Important-mail watcher", what: "Watches Gmail and files important mail with a short explanation.", key: "memcoreMail", def: true, dependsOn: "memcore" }),
  bool({ id: "mail-ai", group: "Notifications", title: "AI helps explain mail (cloud)", what: "For mail the rules do not recognise, a free model writes the explanation from a redacted extract.", key: "memcoreMailPlain", def: false, risk: "cloud", dependsOn: "memcoreMail" }),
  bool({ id: "github-watch", group: "Notifications", title: "GitHub build watcher", what: "Tells you when a build fails and when it is fixed.", key: "memcoreGithubWatch", def: true, dependsOn: "memcore" }),
  bool({ id: "deploy-watch", group: "Notifications", title: "Deploy watcher", what: "Vercel, Render, Netlify and Supabase status.", key: "memcoreDeployWatch", def: true, dependsOn: "memcore" }),
  bool({ id: "wa-watch", group: "Notifications", title: "WhatsApp priority people", what: "Read-only: alerts for people who matter. Needs the WhatsApp bridge.", key: "memcoreWhatsapp", def: false, dependsOn: "memcore" }),
  bool({ id: "wa-speak", group: "Notifications", title: "Read WhatsApp messages aloud (cloud voice)", what: "Speaking message text sends it to the cloud voice service.", key: "memcoreWhatsappSpeak", def: false, risk: "cloud", dependsOn: "memcoreWhatsapp" }),
  bool({ id: "notify-native", group: "Notifications", title: "Windows notifications", what: "Shows alerts in the Windows notification area.", key: "notifyNative", def: true }),
  bool({ id: "notify-toast", group: "Notifications", title: "NEXUS toast", what: "The small NEXUS card for alerts.", key: "notifyToast", def: true }),
  bool({ id: "notify-urgent", group: "Notifications", title: "Urgent alerts break through", what: "Critical alerts are spoken even when busy.", key: "notifyUrgent", def: true }),
  bool({ id: "cr-open", group: "Notifications", title: "CodeRabbit: open failing review", what: "Opens the review when a PR check fails.", key: "coderabbitAutoOpen", def: true }),
  bool({ id: "cr-review", group: "Notifications", title: "CodeRabbit: review automatically", what: "Runs a review on new PRs. Sends the diff to a review service.", key: "coderabbitAutoReview", def: false, risk: "cloud" }),

  // ── Agents ──
  bool({ id: "agent-monitor", group: "Agents", title: "Show running agents", what: "Agent Island: logos of your coding agents at the top of the screen. Keeps the screen awake while they work.", key: "agentMonitor", def: true }),
  bool({ id: "agent-hover", group: "Agents", title: "Agent Island on hover", what: "Hovering shows each agent's detail.", key: "agentMonitorHover", def: true, dependsOn: "agentMonitor" }),
  bool({ id: "agent-hook", group: "Agents", title: "Connect coding agents automatically", what: "Finds Claude Code, Codex, Antigravity and others and connects them. A backup of each one's settings is made first.", key: "agentAutoHook", def: true, risk: "desktop" }),
  bool({ id: "agent-hide-watched", group: "Agents", title: "Hide agents NEXUS is already watching", what: "Keeps the island tidy.", key: "agentIslandHideWatched", def: true, dependsOn: "agentMonitor" }),
  {
    id: "watch-power",
    group: "Agents",
    title: "Shut down when an agent finishes",
    what: "off, practice (dry run, never powers off) or on (announces a 30 second countdown, then shuts down).",
    key: "watchPower",
    kind: "enum",
    options: ["off", "dry", "on"],
    def: "off",
    apply: "live",
    risk: "desktop",
    safety: true,
  },

  // ── System ──
  { id: "autostart", group: "System", title: "Start with Windows", what: "Starts NEXUS when you sign in.", key: "autostart", kind: "bool", def: true, apply: "trigger", risk: "safe", keepInSafeMode: true },
  { id: "early-hello", group: "System", title: "Hello at sign-in", what: "A short greeting early at logon.", key: "earlyHello", kind: "bool", def: true, apply: "restart", risk: "safe" },
  bool({ id: "power-commands", group: "System", title: "Voice power commands", what: "\"Shut down\", \"restart\", \"good night\" (always asks first).", key: "powerCommands", def: true, risk: "desktop" }),
];

// ───────────────────────── logic (pure, tested) ─────────────────────────

export type SettingsMap = Record<string, unknown>;

/** The value the backend is using: the stored value if it has the right type, else the documented default. */
export function effectiveValue(settings: SettingsMap, f: Feature): boolean | string {
  const v = settings[f.key];
  if (f.kind === "bool") return typeof v === "boolean" ? v : (f.def as boolean);
  return typeof v === "string" && f.options?.includes(v) ? v : (f.def as string);
}

/** On means: a boolean that is true, or an enum that is not its first (off) option. */
export function isOn(settings: SettingsMap, f: Feature): boolean {
  const v = effectiveValue(settings, f);
  return f.kind === "bool" ? v === true : v !== (f.options?.[0] ?? "off");
}

/** The parent a feature needs, when that parent is off (the row is shown but inert). */
export function blockedBy(settings: SettingsMap, f: Feature, all: Feature[] = FEATURES): Feature | null {
  if (!f.dependsOn) return null;
  const parent = all.find((p) => p.key === f.dependsOn);
  return parent && !isOn(settings, parent) ? parent : null;
}

/** The patch that switches one feature (bool: to `on`; enum: to the given option). */
export function patchFor(f: Feature, value: boolean | string): SettingsMap {
  return { [f.key]: value };
}

/** Safe mode: switch off everything that listens, watches, acts on the desktop or sends data out, except the way back. */
export function safeModePatch(settings: SettingsMap, all: Feature[] = FEATURES): SettingsMap {
  const patch: SettingsMap = {};
  for (const f of all) {
    if (f.risk === "safe" || f.keepInSafeMode) continue;
    if (!isOn(settings, f)) continue;
    patch[f.key] = f.kind === "bool" ? false : (f.options?.[0] ?? "off");
  }
  return patch;
}

/** Every switch back to its documented default, except the protective ones (Voice Lock, local-only speech, wake check):
 *  "restore defaults" must never quietly lower a protection the owner chose. */
export function defaultsPatch(all: Feature[] = FEATURES): SettingsMap {
  const patch: SettingsMap = {};
  for (const f of all) {
    if (f.keepInSafeMode && f.risk !== "safe") continue;
    patch[f.key] = f.def;
  }
  return patch;
}

export type Filter = "all" | "on" | "off" | "risky";

export function matches(settings: SettingsMap, f: Feature, filter: Filter, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q && !`${f.title} ${f.what} ${f.group} ${f.key}`.toLowerCase().includes(q)) return false;
  if (filter === "on") return isOn(settings, f);
  if (filter === "off") return !isOn(settings, f);
  if (filter === "risky") return f.risk !== "safe";
  return true;
}

export const RISK_LABEL: Record<Risk, string> = {
  safe: "",
  mic: "Uses mic or camera",
  cloud: "Sends data to the cloud",
  desktop: "Acts on your desktop",
};

export const APPLY_LABEL: Record<Apply, string> = {
  live: "Applies at once",
  restart: "Applies after restart",
  trigger: "Applies after a moment",
};
