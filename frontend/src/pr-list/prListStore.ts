import { create } from "zustand";

/** A single PR summary for list display. */
export interface PrSummary {
  number: number;
  title: string;
  author: string;
  repo: string;
  state: string;
  created_at: string;
  mergeable?: boolean | null;
  orgName?: string;
  orgLogoUrl?: string;
  authorAvatarUrl?: string;
  itemType?: "pull_request" | "issue";
  accountType?: "organization" | "personal";
  formattedTime?: string;
  dateGroup?: string;
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

export type SubPage = "feed" | "pull_requests" | "issues" | "organizations";
export type AccountFilter = "all" | "organization" | "personal";
export type AttentionFilter = "all" | "pull_requests" | "issues";
export type ThemeMode = "light" | "dark";

interface PrListState {
  visible: boolean;
  repo: string;
  state: string;
  prs: PrSummary[];
  feedItems: GitHubFeedItem[];
  orgs: GitHubOrgCard[];
  loading: boolean;
  actionInProgress: { prNumber: number; action: "merge" | "analyse" } | null;
  subpage: SubPage;
  accountFilter: AccountFilter;
  attentionFilter: AttentionFilter;
  theme: ThemeMode;

  showPrList: (repo: string, state: string, prs: PrSummary[]) => void;
  hide: () => void;
  setLoading: (loading: boolean) => void;
  setActionInProgress: (action: { prNumber: number; action: "merge" | "analyse" } | null) => void;
  removePr: (prNumber: number, repo?: string) => void;
  setSubpage: (subpage: SubPage) => void;
  setAccountFilter: (filter: AccountFilter) => void;
  setAttentionFilter: (filter: AttentionFilter) => void;
  setTheme: (theme: ThemeMode) => void;
  toggleTheme: () => void;
}

export const usePrList = create<PrListState>((set) => ({
  visible: false,
  repo: "",
  state: "",
  prs: [],
  feedItems: SAMPLE_FEED_ITEMS,
  orgs: SAMPLE_ORGS,
  loading: false,
  actionInProgress: null,
  subpage: "feed",
  accountFilter: "all",
  attentionFilter: "all",
  theme: "light", // White Theme default matching screenshot

  showPrList: (repo, state, prs) => {
    // If state is "open", target the dedicated pull_requests subpage directly!
    // If state is "issues" or "organizations", target their respective subpages directly!
    // If state is "feed" or empty, target the feed home view.
    const initialSubpage: SubPage =
      state === "open" || state === "pull_requests"
        ? "pull_requests"
        : state === "issues"
        ? "issues"
        : state === "organizations"
        ? "organizations"
        : "feed";
    set({
      visible: true,
      repo,
      state,
      prs,
      loading: false,
      actionInProgress: null,
      subpage: initialSubpage,
    });
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

  setSubpage: (subpage) => set({ subpage }),
  setAccountFilter: (accountFilter) => set({ accountFilter }),
  setAttentionFilter: (attentionFilter) => set({ attentionFilter }),
  setTheme: (theme) => set({ theme }),
  toggleTheme: () => set((s) => ({ theme: s.theme === "light" ? "dark" : "light" })),
}));
