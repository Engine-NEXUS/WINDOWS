import { create } from "zustand";
import { getThemeMode, type ThemeMode } from "../sidebar/theme";

/** A single PR summary for list display. */
export interface PrSummary {
  number: number;
  title: string;
  author: string;
  repo: string;
  state: string;
  created_at: string;
  mergeable?: boolean | null;
  html_url?: string;
  repo_owner?: string;
  orgName?: string;
  orgLogoUrl?: string;
  authorAvatarUrl?: string;
  itemType?: "pull_request" | "issue";
  accountType?: "organization" | "personal";
  formattedTime?: string;
  dateGroup?: string;
  draft?: boolean;
  labels?: string[];
}

export interface GitHubFeedItem {
  id: string;
  number: number;
  title: string;
  repo: string;
  author: string;
  authorAvatarUrl?: string;
  orgName?: string;
  orgLogoUrl?: string;
  accountType: "organization" | "personal";
  itemType: "pull_request" | "issue";
  state: "open" | "closed" | "merged";
  created_at: string;
  formattedTime: string;
  dateGroup: "Today" | "Yesterday" | "Earlier";
  mergeable?: boolean | null;
}

export interface GitHubOrgCard {
  id: string;
  name: string;
  division: string;
  mentionsCount: number;
  logoBg: string;
  logoText: string;
  avatar_url?: string;
  html_url?: string;
  login?: string;
}

export interface GitHubRepoCard {
  id: number;
  name: string;
  full_name: string;
  owner_login: string;
  owner_avatar_url: string;
  owner_type?: string | null;
  description: string | null;
  private: boolean;
  fork: boolean;
  html_url: string;
  stars_count: number;
  forks_count: number;
  open_issues_count: number;
  language: string | null;
  language_color: string | null;
  updated_at: string;
  pushed_at: string | null;
}

export interface GitHubStarredCard {
  id: number;
  name: string;
  full_name: string;
  owner_login: string;
  owner_avatar_url: string;
  description: string | null;
  stars_count: number;
  forks_count: number;
  language: string | null;
  language_color: string | null;
  html_url: string;
  updated_at: string;
}

export interface GitHubActivityStats {
  commits_pct: number;
  pull_requests_pct: number;
  code_review_pct: number;
  issues_pct: number;
  total_events: number;
}

export interface GitHubDashboardData {
  profile?: {
    login: string;
    name?: string;
    avatar_url?: string;
    html_url?: string;
    public_repos?: number;
    total_private_repos?: number;
    followers?: number;
    following?: number;
  } | null;
  repos: GitHubRepoCard[];
  prs: {
    id: number;
    number: number;
    title: string;
    state: string;
    html_url: string;
    repo_name: string;
    repo_owner: string;
    author: string;
    author_avatar_url: string;
    created_at: string;
    updated_at: string;
    draft: boolean;
    labels: string[];
  }[];
  orgs: {
    id: number;
    login: string;
    name?: string | null;
    avatar_url: string;
    description?: string | null;
    html_url: string;
  }[];
  starred: GitHubStarredCard[];
  activity_stats: GitHubActivityStats;
  last_updated: string;
}

export const SAMPLE_FEED_ITEMS: GitHubFeedItem[] = [
  {
    id: "item-1",
    number: 34,
    title: "chore: upgrade dependencies to v3",
    repo: "design-system",
    author: "maya",
    authorAvatarUrl: "https://images.unsplash.com/photo-1534528741775-53994a69daeb?w=64&h=64&fit=crop&crop=faces",
    orgName: "design-system",
    orgLogoUrl: "https://images.unsplash.com/photo-1618005182384-a83a8bd57fbe?w=64&h=64&fit=crop",
    accountType: "organization",
    itemType: "pull_request",
    state: "open",
    created_at: new Date(Date.now() - 3600000 * 2).toISOString(),
    formattedTime: "11:25 AM",
    dateGroup: "Today",
    mergeable: true,
  },
  {
    id: "item-2",
    number: 35,
    title: "fix: resolve auth redirect loop",
    repo: "frontend-app",
    author: "sarah",
    authorAvatarUrl: "https://images.unsplash.com/photo-1494790108377-be9c29b29330?w=64&h=64&fit=crop&crop=faces",
    orgName: "frontend-app",
    orgLogoUrl: "https://images.unsplash.com/photo-1550745165-9bc0b252726f?w=64&h=64&fit=crop",
    accountType: "organization",
    itemType: "pull_request",
    state: "open",
    created_at: new Date(Date.now() - 3600000 * 3).toISOString(),
    formattedTime: "10:25 AM",
    dateGroup: "Today",
    mergeable: true,
  },
  {
    id: "item-3",
    number: 36,
    title: "bug: login session expires unexpectedly",
    repo: "frontend-app",
    author: "sarah",
    authorAvatarUrl: "https://images.unsplash.com/photo-1494790108377-be9c29b29330?w=64&h=64&fit=crop&crop=faces",
    orgName: "frontend-app",
    orgLogoUrl: "https://images.unsplash.com/photo-1550745165-9bc0b252726f?w=64&h=64&fit=crop",
    accountType: "organization",
    itemType: "issue",
    state: "open",
    created_at: new Date(Date.now() - 3600000 * 5).toISOString(),
    formattedTime: "8:25 AM",
    dateGroup: "Today",
  },
  {
    id: "item-4",
    number: 37,
    title: "feat(db-pool): Semaphore bounded connection leasing",
    repo: "ultron",
    author: "lakshya",
    authorAvatarUrl: "https://images.unsplash.com/photo-1507003211169-0a1dd7228f2d?w=64&h=64&fit=crop&crop=faces",
    accountType: "personal",
    itemType: "pull_request",
    state: "open",
    created_at: new Date(Date.now() - 86400000).toISOString(),
    formattedTime: "Yesterday",
    dateGroup: "Yesterday",
    mergeable: true,
  },
  {
    id: "item-5",
    number: 38,
    title: "refactor(tts): Unified zero-emotion neural engine & pitch locking",
    repo: "ultron",
    author: "chitkul",
    authorAvatarUrl: "https://images.unsplash.com/photo-1500648767791-00dcc994a43e?w=64&h=64&fit=crop&crop=faces",
    accountType: "personal",
    itemType: "pull_request",
    state: "open",
    created_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    formattedTime: "Oct 16",
    dateGroup: "Earlier",
    mergeable: true,
  },
];

export const SAMPLE_ORGS: GitHubOrgCard[] = [
  {
    id: "org-1",
    name: "Team A",
    division: "Office",
    mentionsCount: 2,
    logoBg: "#0969da",
    logoText: "TA",
  },
  {
    id: "org-2",
    name: "Team B",
    division: "Edu",
    mentionsCount: 2,
    logoBg: "#8250df",
    logoText: "TB",
  },
];

export const SAMPLE_REPOS: GitHubRepoCard[] = [
  {
    id: 101,
    name: "ultron",
    full_name: "lakshya/ultron",
    owner_login: "lakshya",
    owner_avatar_url: "https://avatars.githubusercontent.com/u/1024025",
    description: "Autonomous multimodal AI desktop twin, command center, and voice neural interface.",
    private: false,
    fork: false,
    html_url: "https://github.com/lakshya/ultron",
    stars_count: 342,
    forks_count: 48,
    open_issues_count: 3,
    language: "Rust",
    language_color: "#dea584",
    updated_at: new Date(Date.now() - 3600000 * 2).toISOString(),
    pushed_at: new Date(Date.now() - 3600000 * 2).toISOString(),
  },
  {
    id: 102,
    name: "frontend-app",
    full_name: "lakshya/frontend-app",
    owner_login: "lakshya",
    owner_avatar_url: "https://avatars.githubusercontent.com/u/1024025",
    description: "Next-gen desktop shell with WebGL stardust particles and real-time audio pipeline.",
    private: true,
    fork: false,
    html_url: "https://github.com/lakshya/frontend-app",
    stars_count: 88,
    forks_count: 12,
    open_issues_count: 5,
    language: "TypeScript",
    language_color: "#3178c6",
    updated_at: new Date(Date.now() - 3600000 * 6).toISOString(),
    pushed_at: new Date(Date.now() - 3600000 * 6).toISOString(),
  },
  {
    id: 103,
    name: "design-system",
    full_name: "lakshya/design-system",
    owner_login: "lakshya",
    owner_avatar_url: "https://avatars.githubusercontent.com/u/1024025",
    description: "Shared dark/light tokens, components, and Tailwind twin styling system.",
    private: false,
    fork: false,
    html_url: "https://github.com/lakshya/design-system",
    stars_count: 215,
    forks_count: 30,
    open_issues_count: 2,
    language: "TypeScript",
    language_color: "#3178c6",
    updated_at: new Date(Date.now() - 86400000).toISOString(),
    pushed_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: 104,
    name: "whisper.cpp",
    full_name: "lakshya/whisper.cpp",
    owner_login: "lakshya",
    owner_avatar_url: "https://avatars.githubusercontent.com/u/1024025",
    description: "Port of OpenAI's Whisper model in C/C++ with SIMD optimizations.",
    private: false,
    fork: true,
    html_url: "https://github.com/lakshya/whisper.cpp",
    stars_count: 14,
    forks_count: 4,
    open_issues_count: 0,
    language: "C++",
    language_color: "#f34b7d",
    updated_at: new Date(Date.now() - 86400000 * 3).toISOString(),
    pushed_at: new Date(Date.now() - 86400000 * 3).toISOString(),
  },
];

export const SAMPLE_STARRED: GitHubStarredCard[] = [
  {
    id: 201,
    name: "tauri",
    full_name: "tauri-apps/tauri",
    owner_login: "tauri-apps",
    owner_avatar_url: "https://avatars.githubusercontent.com/u/54438341",
    description: "Build smaller, faster, and more secure desktop applications with a web frontend.",
    stars_count: 82400,
    forks_count: 2400,
    language: "Rust",
    language_color: "#dea584",
    html_url: "https://github.com/tauri-apps/tauri",
    updated_at: new Date(Date.now() - 3600000 * 4).toISOString(),
  },
  {
    id: 202,
    name: "zustand",
    full_name: "pmndrs/zustand",
    owner_login: "pmndrs",
    owner_avatar_url: "https://avatars.githubusercontent.com/u/45790596",
    description: "Bear necessities for state management in React — small, fast and scalable barebones state-management solution.",
    stars_count: 49800,
    forks_count: 1600,
    language: "TypeScript",
    language_color: "#3178c6",
    html_url: "https://github.com/pmndrs/zustand",
    updated_at: new Date(Date.now() - 3600000 * 8).toISOString(),
  },
  {
    id: 203,
    name: "tokio",
    full_name: "tokio-rs/tokio",
    owner_login: "tokio-rs",
    owner_avatar_url: "https://avatars.githubusercontent.com/u/15294527",
    description: "A runtime for writing reliable, asynchronous, and slim applications with the Rust programming language.",
    stars_count: 27100,
    forks_count: 2500,
    language: "Rust",
    language_color: "#dea584",
    html_url: "https://github.com/tokio-rs/tokio",
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
];

export type SubPage = "feed" | "repos" | "pull_requests" | "issues" | "organizations" | "starred";
export type AccountFilter = "all" | "organization" | "personal";
export type AttentionFilter = "all" | "pull_requests" | "issues";
export type { ThemeMode };

/** Derive title for PR list based on repository name. */
export function derivePrListTitle(repo: string): string {
  const trimmed = repo.trim();
  if (!trimmed || trimmed === "all repositories") {
    return "Open pull requests";
  }
  if (trimmed.startsWith("PR #")) {
    return `${trimmed} in your repositories`;
  }
  return `Open PRs in ${trimmed}`;
}

interface PrListState {
  visible: boolean;
  repo: string;
  state: string;
  prs: PrSummary[];
  pinned: boolean;
  title: string;
  feedItems: GitHubFeedItem[];
  orgs: GitHubOrgCard[];
  repos: GitHubRepoCard[];
  starred: GitHubStarredCard[];
  activityStats: GitHubActivityStats;
  lastUpdated: string | null;
  loading: boolean;
  actionInProgress: { prNumber: number; action: "merge" | "analyse" } | null;
  subpage: SubPage;
  accountFilter: AccountFilter;
  attentionFilter: AttentionFilter;
  theme: ThemeMode;
  isAuthenticated: boolean;
  isAuthenticating: boolean;
  authChecked: boolean;
  authError: string | null;
  currentUser: string;
  profile: { login: string; name?: string; avatar_url?: string } | null;

  showPrList: (repo: string, state: string, prs: PrSummary[]) => void;
  clearPinned: () => void;
  showFeedUnlessPinned: () => void;
  hide: () => void;
  setLoading: (loading: boolean) => void;
  setActionInProgress: (action: { prNumber: number; action: "merge" | "analyse" } | null) => void;
  removePr: (prNumber: number, repo?: string) => void;
  setSubpage: (subpage: SubPage) => void;
  setAccountFilter: (filter: AccountFilter) => void;
  setAttentionFilter: (filter: AttentionFilter) => void;
  setTheme: (theme: ThemeMode) => void;
  setAuthenticated: (authenticated: boolean) => void;
  setAuthenticating: (authenticating: boolean) => void;
  setAuthChecked: (checked: boolean) => void;
  setCurrentUser: (user: string) => void;
  setAuthError: (error: string | null) => void;
  setRepos: (repos: GitHubRepoCard[]) => void;
  setStarred: (starred: GitHubStarredCard[]) => void;
  setActivityStats: (stats: GitHubActivityStats) => void;
  loadDashboardData: (forceRefresh?: boolean) => Promise<void>;
  applyDashboardData: (data: GitHubDashboardData) => void;
}

const initialAuthed =
  typeof window !== "undefined"
    ? localStorage.getItem("nexus_github_authed") === "true" ||
      !!localStorage.getItem("nexus_github_login")
    : false;
const initialUser =
  (typeof window !== "undefined" && localStorage.getItem("nexus_github_login")) ||
  "ChitkulLakshya";

export const usePrList = create<PrListState>((set, get) => ({
  visible: true,
  repo: "",
  state: "",
  prs: [],
  pinned: false,
  title: "",
  feedItems: SAMPLE_FEED_ITEMS,
  orgs: SAMPLE_ORGS,
  repos: SAMPLE_REPOS,
  starred: SAMPLE_STARRED,
  activityStats: {
    commits_pct: 92,
    pull_requests_pct: 7,
    code_review_pct: 0,
    issues_pct: 1,
    total_events: 100,
  },
  lastUpdated: null,
  loading: false,
  actionInProgress: null,
  subpage: "feed",
  accountFilter: "all",
  attentionFilter: "all",
  theme: getThemeMode(), // Sync directly with Settings window theme
  isAuthenticated: initialAuthed,
  isAuthenticating: false,
  authChecked: initialAuthed,
  authError: null,
  currentUser: initialUser,
  profile: null,

  showPrList: (repo, state, prs) => {
    const initialSubpage: SubPage =
      state === "open" || state === "pull_requests"
        ? "pull_requests"
        : state === "issues"
        ? "issues"
        : state === "organizations"
        ? "organizations"
        : state === "repos"
        ? "repos"
        : state === "starred"
        ? "starred"
        : "feed";
    set({
      visible: true,
      repo,
      state,
      prs: prs || [],
      loading: false,
      actionInProgress: null,
      subpage: initialSubpage,
      pinned: true,
      title: derivePrListTitle(repo),
    });
  },

  clearPinned: () => set({ pinned: false, title: "" }),

  // `sidebar:set_view` can arrive after the list it announces (the list rides
  // `orchestrator:event`, the view switch a separate event). Landing on the
  // feed there would wipe a list the user just asked for, so a pinned list
  // that names a repo is kept.
  showFeedUnlessPinned: () => {
    const s = get();
    if (s.pinned && s.repo !== "") {
      set({ visible: true });
      return;
    }
    get().showPrList("", "feed", []);
  },

  hide: () => set({ visible: false, prs: [], actionInProgress: null }),

  setLoading: (loading) => set({ loading }),

  setActionInProgress: (action) => set({ actionInProgress: action }),

  removePr: (prNumber, repo) =>
    set((s) => ({
      prs: s.prs.filter((p) =>
        repo ? !(p.number === prNumber && p.repo === repo) : p.number !== prNumber
      ),
      feedItems: s.feedItems.filter((i) =>
        repo ? !(i.number === prNumber && i.repo === repo) : i.number !== prNumber
      ),
    })),

  setSubpage: (subpage) =>
    set({
      subpage,
      pinned: subpage === "pull_requests" ? get().pinned : false,
    }),
  setAccountFilter: (accountFilter) => set({ accountFilter }),
  setAttentionFilter: (attentionFilter) => set({ attentionFilter }),
  setTheme: (theme) => set({ theme }),
  setAuthenticated: (isAuthenticated) => {
    if (typeof window !== "undefined") {
      if (isAuthenticated) localStorage.setItem("nexus_github_authed", "true");
      else localStorage.removeItem("nexus_github_authed");
    }
    set({ isAuthenticated, authChecked: true });
  },
  setAuthenticating: (isAuthenticating) => set({ isAuthenticating }),
  setAuthChecked: (authChecked) => set({ authChecked }),
  setCurrentUser: (currentUser) => {
    if (typeof window !== "undefined") {
      localStorage.setItem("nexus_github_login", currentUser);
    }
    set({ currentUser });
  },
  setAuthError: (authError) => set({ authError }),
  setRepos: (repos) => set({ repos }),
  setStarred: (starred) => set({ starred }),
  setActivityStats: (activityStats) => set({ activityStats }),

  applyDashboardData: (data) => {
    const prs: PrSummary[] = (data.prs || []).map((p) => {
      const repoMatch = (data.repos || []).find(
        (r) =>
          r.name.toLowerCase() === p.repo_name.toLowerCase() ||
          r.full_name.toLowerCase().endsWith("/" + p.repo_name.toLowerCase())
      );
      const orgMatch = (data.orgs || []).find(
        (o) =>
          (o.login && o.login.toLowerCase() === p.repo_owner.toLowerCase()) ||
          (o.name && o.name.toLowerCase() === p.repo_owner.toLowerCase())
      );
      const orgLogoUrl =
        repoMatch?.owner_avatar_url ||
        orgMatch?.avatar_url ||
        (p.repo_owner ? `https://github.com/${p.repo_owner}.png` : undefined);
      const authorAvatarUrl =
        p.author_avatar_url ||
        (p.author ? `https://github.com/${p.author}.png` : undefined);

      return {
        number: p.number,
        title: p.title,
        author: p.author,
        repo: p.repo_name,
        repo_owner: p.repo_owner,
        state: p.state,
        created_at: p.created_at,
        html_url: p.html_url,
        orgLogoUrl,
        authorAvatarUrl,
        itemType: "pull_request" as const,
        accountType:
          orgMatch || (repoMatch && repoMatch.owner_type === "Organization")
            ? ("organization" as const)
            : ("personal" as const),
        draft: p.draft,
        labels: p.labels,
      };
    });

    const orgs: GitHubOrgCard[] = data.orgs.map((o) => ({
      id: String(o.id),
      name: o.name || o.login,
      division: o.description || "Organization",
      mentionsCount: 0,
      logoBg: "#0969da",
      logoText: o.login.slice(0, 2).toUpperCase(),
      avatar_url: o.avatar_url,
      html_url: o.html_url,
      login: o.login,
    }));

    if (data.profile?.login) {
      if (typeof window !== "undefined") {
        localStorage.setItem("nexus_github_authed", "true");
        localStorage.setItem("nexus_github_login", data.profile.login);
      }
    }

    set({
      repos: data.repos && data.repos.length > 0 ? data.repos : get().repos,
      starred: data.starred && data.starred.length > 0 ? data.starred : get().starred,
      activityStats: data.activity_stats || get().activityStats,
      prs: get().pinned ? get().prs : (prs.length > 0 ? prs : get().prs),
      orgs: orgs.length > 0 ? orgs : get().orgs,
      profile: data.profile || get().profile,
      currentUser: data.profile?.login || get().currentUser,
      lastUpdated: data.last_updated,
      isAuthenticated: true,
      authChecked: true,
    });
  },

  loadDashboardData: async (forceRefresh = false) => {
    if (typeof (window as any).__TAURI_INTERNALS__ === "undefined") {
      return;
    }
    set({ loading: true });
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const serverUrl = localStorage.getItem("nexus_server_url") || "";
      const data = await invoke<GitHubDashboardData>("github_get_dashboard_data", {
        serverUrl,
        userId: "local-user",
        forceRefresh,
      });
      if (data) {
        get().applyDashboardData(data);
      }
    } catch (err) {
      console.warn("[GitHub Dashboard] Failed to load dashboard data:", err);
    } finally {
      set({ loading: false });
    }
  },
}));
