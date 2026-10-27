import { describe, it, expect, beforeEach } from "vitest";
import { usePrList } from "./prListStore";

describe("GitHub Dashboard (prListStore)", () => {
  beforeEach(() => {
    usePrList.setState({
      visible: false,
      repo: "",
      state: "",
      prs: [],
      pinned: false,
      title: "",
      subpage: "feed",
      accountFilter: "all",
      attentionFilter: "all",
      theme: "light",
      actionInProgress: null,
      isAuthenticated: false,
      isAuthenticating: false,
      authError: null,
    });
  });

  it("a late sidebar:set_view does not wipe a pinned list that already arrived", () => {
    const { showPrList, showFeedUnlessPinned } = usePrList.getState();
    showPrList("acme/zync", "open", [{ number: 7, title: "x", repo: "acme/zync" } as any]);
    showFeedUnlessPinned();
    const s = usePrList.getState();
    expect(s.prs.length).toBe(1);
    expect(s.repo).toBe("acme/zync");
    expect(s.pinned).toBe(true);
  });

  it("set_view with nothing pinned still lands on the feed", () => {
    usePrList.getState().showFeedUnlessPinned();
    const s = usePrList.getState();
    expect(s.visible).toBe(true);
    expect(s.subpage).toBe("feed");
    expect(s.repo).toBe("");
  });

  it("an empty pinned list (no PR found) is also kept", () => {
    const { showPrList, showFeedUnlessPinned } = usePrList.getState();
    showPrList("acme/zync", "open", []);
    showFeedUnlessPinned();
    expect(usePrList.getState().repo).toBe("acme/zync");
  });

  it("defaults to logged-out state with sign-in gate required", () => {
    const s = usePrList.getState();
    expect(s.isAuthenticated).toBe(false);
    expect(s.isAuthenticating).toBe(false);
    expect(s.authError).toBeNull();
  });

  it("updates authentication state when user signs in or out", () => {
    const { setAuthenticated, setAuthenticating, setAuthError } = usePrList.getState();

    setAuthenticating(true);
    expect(usePrList.getState().isAuthenticating).toBe(true);

    setAuthenticated(true);
    setAuthenticating(false);
    expect(usePrList.getState().isAuthenticated).toBe(true);
    expect(usePrList.getState().isAuthenticating).toBe(false);

    setAuthError("Failed to connect");
    expect(usePrList.getState().authError).toBe("Failed to connect");

    setAuthenticated(false);
    expect(usePrList.getState().isAuthenticated).toBe(false);
  });

  it("defaults to light theme (matching White Canvas requirement) and feed subpage", () => {
    const s = usePrList.getState();
    expect(s.theme).toBe("light");
    expect(s.subpage).toBe("feed");
    expect(s.accountFilter).toBe("all");
    expect(s.attentionFilter).toBe("all");
  });

  it("updates theme to match Settings window theme mode", () => {
    const { setTheme } = usePrList.getState();
    setTheme("dark");
    expect(usePrList.getState().theme).toBe("dark");

    setTheme("light");
    expect(usePrList.getState().theme).toBe("light");
  });

  it("switches subpage when voice intent specifies 'open' vs 'feed'", () => {
    // When user says "open the pr list" (state: "open"), subpage should automatically be "pull_requests"
    usePrList.getState().showPrList("ultron", "open", []);
    expect(usePrList.getState().visible).toBe(true);
    expect(usePrList.getState().subpage).toBe("pull_requests");

    // When user says "show my github feed" (state: "feed"), subpage should automatically be "feed"
    usePrList.getState().showPrList("ultron", "feed", []);
    expect(usePrList.getState().subpage).toBe("feed");
  });

  it("supports switching account filters across all, organization, and personal", () => {
    const { setAccountFilter } = usePrList.getState();
    setAccountFilter("organization");
    expect(usePrList.getState().accountFilter).toBe("organization");

    setAccountFilter("personal");
    expect(usePrList.getState().accountFilter).toBe("personal");

    setAccountFilter("all");
    expect(usePrList.getState().accountFilter).toBe("all");
  });

  it("removes PR from list when merged or closed", () => {
    const { showPrList, removePr } = usePrList.getState();
    showPrList("ultron", "open", [
      { number: 101, title: "Test PR 1", author: "alice", repo: "ultron", state: "open", created_at: "" },
      { number: 102, title: "Test PR 2", author: "bob", repo: "ultron", state: "open", created_at: "" },
    ]);
    expect(usePrList.getState().prs.length).toBe(2);

    removePr(101, "ultron");
    expect(usePrList.getState().prs.length).toBe(1);
    expect(usePrList.getState().prs[0].number).toBe(102);
  });

  it("switches to repos and starred subpages", () => {
    const { setSubpage, showPrList } = usePrList.getState();
    setSubpage("repos");
    expect(usePrList.getState().subpage).toBe("repos");

    setSubpage("starred");
    expect(usePrList.getState().subpage).toBe("starred");

    showPrList("", "repos", []);
    expect(usePrList.getState().subpage).toBe("repos");

    showPrList("", "starred", []);
    expect(usePrList.getState().subpage).toBe("starred");
  });

  it("applies real GitHub dashboard data into the store", () => {
    const { applyDashboardData } = usePrList.getState();
    applyDashboardData({
      profile: { login: "testuser" },
      repos: [
        {
          id: 1,
          name: "my-repo",
          full_name: "testuser/my-repo",
          owner_login: "testuser",
          owner_avatar_url: "",
          description: "A great repo",
          private: false,
          fork: false,
          html_url: "https://github.com/testuser/my-repo",
          stars_count: 42,
          forks_count: 5,
          open_issues_count: 1,
          language: "Rust",
          language_color: "#dea584",
          updated_at: "2026-10-18T12:00:00Z",
          pushed_at: null,
        },
      ],
      prs: [
        {
          id: 11,
          number: 1,
          title: "Initial PR",
          state: "open",
          html_url: "https://github.com/testuser/my-repo/pull/1",
          repo_name: "my-repo",
          repo_owner: "testuser",
          author: "testuser",
          author_avatar_url: "",
          created_at: "2026-10-18T10:00:00Z",
          updated_at: "2026-10-18T10:00:00Z",
          draft: false,
          labels: ["enhancement"],
        },
      ],
      orgs: [
        {
          id: 99,
          login: "my-org",
          name: "My Org",
          avatar_url: "https://github.com/my-org.png",
          description: "Org description",
          html_url: "https://github.com/my-org",
        },
      ],
      starred: [
        {
          id: 501,
          name: "awesome-rust",
          full_name: "rust-unofficial/awesome-rust",
          owner_login: "rust-unofficial",
          owner_avatar_url: "",
          description: "A curated list of Rust code and resources.",
          stars_count: 40000,
          forks_count: 2000,
          language: "Rust",
          language_color: "#dea584",
          html_url: "https://github.com/rust-unofficial/awesome-rust",
          updated_at: "2026-10-18T08:00:00Z",
        },
      ],
      activity_stats: {
        commits_pct: 85,
        pull_requests_pct: 10,
        code_review_pct: 3,
        issues_pct: 2,
        total_events: 50,
      },
      last_updated: "2026-10-18T12:00:00Z",
    });

    const s = usePrList.getState();
    expect(s.isAuthenticated).toBe(true);
    expect(s.authChecked).toBe(true);
    expect(s.currentUser).toBe("testuser");
    expect(s.repos.length).toBe(1);
    expect(s.repos[0].name).toBe("my-repo");
    expect(s.prs.length).toBe(1);
    expect(s.prs[0].repo).toBe("my-repo");
    expect(s.orgs.length).toBe(1);
    expect(s.orgs[0].name).toBe("My Org");
    expect(s.starred.length).toBe(1);
    expect(s.starred[0].name).toBe("awesome-rust");
    expect(s.activityStats.commits_pct).toBe(85);
  });

  it("handles authChecked and currentUser tracking accurately", () => {
    const { setAuthChecked, setCurrentUser } = usePrList.getState();
    setAuthChecked(false);
    expect(usePrList.getState().authChecked).toBe(false);

    setAuthChecked(true);
    expect(usePrList.getState().authChecked).toBe(true);

    setCurrentUser("ChitkulLakshya");
    expect(usePrList.getState().currentUser).toBe("ChitkulLakshya");
  });

  it("derives title for the three cases (all repositories/empty, PR #, specific repo)", () => {
    const { showPrList } = usePrList.getState();

    // 1. All repositories / empty
    showPrList("all repositories", "open", []);
    expect(usePrList.getState().title).toBe("Open pull requests");
    showPrList("", "open", []);
    expect(usePrList.getState().title).toBe("Open pull requests");

    // 2. PR #
    showPrList("PR #34", "open", []);
    expect(usePrList.getState().title).toBe("PR #34 in your repositories");

    // 3. Specific repo
    showPrList("zync", "open", []);
    expect(usePrList.getState().title).toBe("Open PRs in zync");
  });

  it("pinned list survives applyDashboardData", () => {
    const { showPrList, applyDashboardData } = usePrList.getState();
    const pinnedPr = {
      number: 34,
      title: "Pinned PR #34",
      author: "alice",
      repo: "zync",
      state: "open",
      created_at: "2026-10-25T12:00:00Z",
    };
    showPrList("PR #34", "open", [pinnedPr]);
    expect(usePrList.getState().pinned).toBe(true);
    expect(usePrList.getState().prs).toEqual([pinnedPr]);

    // Apply dashboard data with a completely different PR
    applyDashboardData({
      repos: [
        {
          id: 1,
          name: "zync",
          full_name: "chitkul/zync",
          owner_login: "chitkul",
          owner_avatar_url: "",
          description: null,
          private: false,
          fork: false,
          html_url: "",
          stars_count: 5,
          forks_count: 0,
          open_issues_count: 1,
          language: "Rust",
          language_color: null,
          updated_at: "",
          pushed_at: null,
        },
      ],
      prs: [
        {
          id: 99,
          number: 99,
          title: "Dashboard PR",
          state: "open",
          html_url: "",
          repo_name: "zync",
          repo_owner: "chitkul",
          author: "bob",
          author_avatar_url: "",
          created_at: "",
          updated_at: "",
          draft: false,
          labels: [],
        },
      ],
      orgs: [],
      starred: [],
      activity_stats: {
        commits_pct: 100,
        pull_requests_pct: 0,
        code_review_pct: 0,
        issues_pct: 0,
        total_events: 10,
      },
      last_updated: "2026-10-25T13:00:00Z",
    });

    const s = usePrList.getState();
    expect(s.pinned).toBe(true);
    // PRs must NOT be overwritten while pinned
    expect(s.prs).toEqual([pinnedPr]);
    // Other data like repos and activity stats ARE updated
    expect(s.repos.length).toBe(1);
    expect(s.repos[0].name).toBe("zync");
    expect(s.lastUpdated).toBe("2026-10-25T13:00:00Z");
  });

  it("empty pinned result replaces a previous list", () => {
    const { showPrList } = usePrList.getState();
    // Start with a non-empty list
    showPrList("ultron", "open", [
      {
        number: 10,
        title: "Old PR",
        author: "alice",
        repo: "ultron",
        state: "open",
        created_at: "",
      },
    ]);
    expect(usePrList.getState().prs.length).toBe(1);

    // Call showPrList with empty prs
    showPrList("PR #34", "open", []);
    const s = usePrList.getState();
    expect(s.pinned).toBe(true);
    expect(s.prs).toEqual([]);
    expect(s.title).toBe("PR #34 in your repositories");
    expect(s.subpage).toBe("pull_requests");
  });

  it("unpin on subpage change", () => {
    const { showPrList, setSubpage, clearPinned } = usePrList.getState();
    showPrList("PR #34", "open", []);
    expect(usePrList.getState().pinned).toBe(true);
    expect(usePrList.getState().subpage).toBe("pull_requests");

    // Switching to another subpage clears pinned
    setSubpage("feed");
    expect(usePrList.getState().pinned).toBe(false);

    // Re-pinning then calling clearPinned directly
    showPrList("PR #34", "open", []);
    expect(usePrList.getState().pinned).toBe(true);
    clearPinned();
    expect(usePrList.getState().pinned).toBe(false);
    expect(usePrList.getState().title).toBe("");
  });
});
