import { describe, it, expect, beforeEach } from "vitest";
import { usePrList } from "./prListStore";

describe("GitHub Dashboard (prListStore)", () => {
  beforeEach(() => {
    usePrList.setState({
      visible: false,
      repo: "",
      state: "",
      prs: [],
      subpage: "feed",
      accountFilter: "all",
      attentionFilter: "all",
      theme: "light",
      actionInProgress: null,
    });
  });

  it("defaults to light theme (matching White Canvas requirement) and feed subpage", () => {
    const s = usePrList.getState();
    expect(s.theme).toBe("light");
    expect(s.subpage).toBe("feed");
    expect(s.accountFilter).toBe("all");
    expect(s.attentionFilter).toBe("all");
  });

  it("toggles theme smoothly between light and dark", () => {
    const { toggleTheme } = usePrList.getState();
    toggleTheme();
    expect(usePrList.getState().theme).toBe("dark");

    toggleTheme();
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
});
