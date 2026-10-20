import { describe, expect, it } from "vitest";
import {
  buildAccounts,
  countReadyMcp,
  formatCount,
  initialOf,
  requestsSubtitle,
  type GoogleAccountProfile,
  type GithubProfile,
  type McpCard,
} from "./hubModel";

describe("buildAccounts boundary conditions", () => {
  it("handles empty arrays, undefined pictures, and missing names", () => {
    const google: GoogleAccountProfile[] = [
      { email: "user@example.com", name: "", is_primary: false, picture: null },
    ];
    const rows = buildAccounts(google, false, null);
    expect(rows).toHaveLength(1);
    expect(rows[0].name).toBe("user@example.com");
    expect(rows[0].picture).toBeUndefined();
    expect(rows[0].handle).toBe("user@example.com");
    expect(rows[0].primary).toBe(false);
  });

  it("places multiple primary accounts first while preserving remaining order", () => {
    const google: GoogleAccountProfile[] = [
      { email: "second@x.com", name: "Second", is_primary: false },
      { email: "first-primary@x.com", name: "P1", is_primary: true },
      { email: "third@x.com", name: "Third", is_primary: false },
      { email: "second-primary@x.com", name: "P2", is_primary: true },
    ];
    const rows = buildAccounts(google, false, null);
    expect(rows.map((r) => r.id)).toEqual([
      "first-primary@x.com",
      "second-primary@x.com",
      "second@x.com",
      "third@x.com",
    ]);
  });

  it("handles GitHub connected with missing name falling back to login", () => {
    const gh: GithubProfile = { login: "devuser", name: "", avatar_url: "https://avatar/gh" };
    const rows = buildAccounts([], true, gh);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      provider: "github",
      id: "devuser",
      name: "devuser",
      handle: "@devuser",
      picture: "https://avatar/gh",
      primary: false,
    });
  });

  it("handles GitHub connected with null profile", () => {
    const rows = buildAccounts([], true, null);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      provider: "github",
      id: "github",
      name: "GitHub",
      handle: "GitHub account",
      picture: undefined,
      primary: false,
    });
  });

  it("handles unicode and accented names in accounts", () => {
    const google: GoogleAccountProfile[] = [
      { email: "ren@example.com", name: "René François", is_primary: true },
    ];
    const rows = buildAccounts(google, false, null);
    expect(rows[0].name).toBe("René François");
  });
});

describe("initialOf boundary conditions", () => {
  it("extracts and capitalizes the first visible letter", () => {
    expect(initialOf({ name: "alice", handle: "a@x.com" })).toBe("A");
    expect(initialOf({ name: "  bob  ", handle: "b@x.com" })).toBe("B");
    expect(initialOf({ name: "@charlie", handle: "c@x.com" })).toBe("C");
  });

  it("handles fallback to handle when name is empty", () => {
    expect(initialOf({ name: "", handle: "@octocat" })).toBe("O");
    expect(initialOf({ name: "", handle: "octocat" })).toBe("O");
  });

  it("returns '?' when name and handle are both empty or whitespace only", () => {
    expect(initialOf({ name: "", handle: "" })).toBe("?");
    expect(initialOf({ name: "   ", handle: "  " })).toBe("?");
    expect(initialOf({ name: "@", handle: "@" })).toBe("?");
  });

  it("handles unicode characters correctly", () => {
    expect(initialOf({ name: "éowyn", handle: "" })).toBe("É");
    expect(initialOf({ name: "λ-user", handle: "" })).toBe("Λ");
  });
});

describe("countReadyMcp boundary conditions", () => {
  it("returns 0 for null, undefined, or empty arrays", () => {
    expect(countReadyMcp(null)).toBe(0);
    expect(countReadyMcp(undefined)).toBe(0);
    expect(countReadyMcp([])).toBe(0);
  });

  it("is case-sensitive to 'ready' and ignores other states", () => {
    const cards: McpCard[] = [
      { server: "A", state: "ready" },
      { server: "B", state: "Ready" }, // uppercase R ignored
      { server: "C", state: "READY" },
      { server: "D", state: "auth_required" },
      { server: "E", state: "down" },
      { server: "F", state: "error" },
    ];
    expect(countReadyMcp(cards)).toBe(1);
  });

  it("counts large numbers of ready cards", () => {
    const cards: McpCard[] = Array.from({ length: 50 }, (_, i) => ({
      server: `srv-${i}`,
      state: "ready",
    }));
    expect(countReadyMcp(cards)).toBe(50);
  });
});

describe("formatCount boundary conditions", () => {
  it("clamps non-finite and negative numbers to '0'", () => {
    expect(formatCount(-1)).toBe("0");
    expect(formatCount(-1000)).toBe("0");
    expect(formatCount(Number.NaN)).toBe("0");
    expect(formatCount(Infinity)).toBe("0");
    expect(formatCount(-Infinity)).toBe("0");
  });

  it("formats counts under 1000 as integers", () => {
    expect(formatCount(0)).toBe("0");
    expect(formatCount(1)).toBe("1");
    expect(formatCount(999)).toBe("999");
    expect(formatCount(999.9)).toBe("999");
  });

  it("formats thousands (1K - 999.9K) stripping trailing .0", () => {
    expect(formatCount(1000)).toBe("1K");
    expect(formatCount(1050)).toBe("1K");
    expect(formatCount(1099)).toBe("1K");
    expect(formatCount(1100)).toBe("1.1K");
    expect(formatCount(1500)).toBe("1.5K");
    expect(formatCount(1999)).toBe("1.9K");
    expect(formatCount(999900)).toBe("999.9K");
  });

  it("formats millions (1M+) stripping trailing .0", () => {
    expect(formatCount(1_000_000)).toBe("1M");
    expect(formatCount(1_049_999)).toBe("1M");
    expect(formatCount(1_050_000)).toBe("1M");
    expect(formatCount(1_100_000)).toBe("1.1M");
    expect(formatCount(1_999_999)).toBe("1.9M");
    expect(formatCount(2_500_000)).toBe("2.5M");
    expect(formatCount(100_000_000)).toBe("100M");
  });
});

describe("requestsSubtitle boundary conditions", () => {
  it("returns empty string for null stats", () => {
    expect(requestsSubtitle(null)).toBe("");
  });

  it("formats zero and large totals cleanly", () => {
    expect(requestsSubtitle({ today: 0, all_time: 0 })).toBe("0 all-time");
    expect(requestsSubtitle({ today: 50, all_time: 45100 })).toBe("45.1K all-time");
    expect(requestsSubtitle({ today: 120, all_time: 2000000 })).toBe("2M all-time");
  });
});
