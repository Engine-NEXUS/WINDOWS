import { describe, it, expect } from "vitest";
import { needsSignin } from "./HubHome";
import type { HubAccount } from "./hubModel";

const acct = (id: string, provider: "google" | "github" = "google"): HubAccount =>
  ({ provider, id, name: id, handle: id, primary: false }) as HubAccount;

describe("needsSignin", () => {
  it("marks only the account that failed", () => {
    const acc = { googleNeedsSignin: true, googleSigninAccounts: ["a@x.com"] };
    expect(needsSignin(acc, acct("a@x.com"))).toBe(true);
    expect(needsSignin(acc, acct("B@x.com"))).toBe(false);
  });
  it("never marks GitHub or healthy states", () => {
    expect(needsSignin({ googleNeedsSignin: true, googleSigninAccounts: ["a@x.com"] }, acct("a@x.com", "github"))).toBe(false);
    expect(needsSignin({ googleNeedsSignin: false, googleSigninAccounts: [] }, acct("a@x.com"))).toBe(false);
  });
  it("falls back to every Google row when the list is unknown", () => {
    expect(needsSignin({ googleNeedsSignin: true, googleSigninAccounts: [] }, acct("a@x.com"))).toBe(true);
  });
});
