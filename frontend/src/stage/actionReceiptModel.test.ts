import { describe, expect, it } from "vitest";
import {
  ActionReceiptModel,
  formatReceiptHeadline,
  handleCancel,
  handleReceipt,
  handleTick,
  INITIAL_ACTION_RECEIPT_STATE,
  isReceiptExpired,
  normalizeReceiptPayload,
  RECEIPT_AUTO_HIDE_MS,
  shouldShowReceipt,
  type ActionReceipt,
  type ActionReceiptState,
} from "./actionReceiptModel";

describe("ActionReceipt model - payload validation & normalization", () => {
  const validCases: Array<{
    name: string;
    input: unknown;
    expected: ActionReceipt;
  }> = [
    {
      name: "open_app success",
      input: {
        kind: "open_app",
        headline: "Opened",
        subject: "Notepad",
        title: null,
        detail: null,
        ok: true,
      },
      expected: {
        kind: "open_app",
        headline: "Opened",
        subject: "Notepad",
        title: null,
        detail: null,
        ok: true,
      },
    },
    {
      name: "open_site success with extra whitespace",
      input: {
        kind: "open_site",
        headline: "  Opened  ",
        subject: "  github.com  ",
        title: null,
        detail: null,
        ok: true,
      },
      expected: {
        kind: "open_site",
        headline: "Opened",
        subject: "github.com",
        title: null,
        detail: null,
        ok: true,
      },
    },
    {
      name: "search with title and detail",
      input: {
        kind: "search",
        headline: "Opened",
        subject: "youtube.com",
        title: "lofi beats",
        detail: "Search results. Nothing is playing yet.",
        ok: true,
      },
      expected: {
        kind: "search",
        headline: "Opened",
        subject: "youtube.com",
        title: "lofi beats",
        detail: "Search results. Nothing is playing yet.",
        ok: true,
      },
    },
    {
      name: "media failure with detail",
      input: {
        kind: "media",
        headline: "Tried to play",
        subject: "spotify",
        title: null,
        detail: "Playback device not found.",
        ok: false,
      },
      expected: {
        kind: "media",
        headline: "Tried to play",
        subject: "spotify",
        title: null,
        detail: "Playback device not found.",
        ok: false,
      },
    },
    {
      name: "empty title and whitespace-only detail normalized to null",
      input: {
        kind: "open_app",
        headline: "Opened",
        subject: "Terminal",
        title: "   ",
        detail: "",
        ok: true,
      },
      expected: {
        kind: "open_app",
        headline: "Opened",
        subject: "Terminal",
        title: null,
        detail: null,
        ok: true,
      },
    },
  ];

  it.each(validCases)("$name", ({ input, expected }) => {
    expect(normalizeReceiptPayload(input)).toEqual(expected);
  });

  const malformedCases: Array<{
    name: string;
    input: unknown;
  }> = [
    { name: "null payload", input: null },
    { name: "undefined payload", input: undefined },
    { name: "number payload", input: 123 },
    { name: "string payload", input: "opened notepad" },
    { name: "array payload", input: [] },
    {
      name: "missing kind",
      input: { headline: "Opened", subject: "Notepad", ok: true },
    },
    {
      name: "invalid kind",
      input: { kind: "invalid_kind", headline: "Opened", subject: "Notepad", ok: true },
    },
    {
      name: "missing headline",
      input: { kind: "open_app", subject: "Notepad", ok: true },
    },
    {
      name: "empty headline",
      input: { kind: "open_app", headline: "   ", subject: "Notepad", ok: true },
    },
    {
      name: "missing subject",
      input: { kind: "open_app", headline: "Opened", ok: true },
    },
    {
      name: "empty subject",
      input: { kind: "open_app", headline: "Opened", subject: "", ok: true },
    },
    {
      name: "missing ok boolean",
      input: { kind: "open_app", headline: "Opened", subject: "Notepad" },
    },
    {
      name: "non-boolean ok (string)",
      input: { kind: "open_app", headline: "Opened", subject: "Notepad", ok: "true" },
    },
    {
      name: "invalid title type (number)",
      input: { kind: "open_app", headline: "Opened", subject: "Notepad", title: 42, ok: true },
    },
    {
      name: "invalid detail type (boolean)",
      input: { kind: "open_app", headline: "Opened", subject: "Notepad", detail: true, ok: true },
    },
  ];

  it.each(malformedCases)("rejects malformed: $name", ({ input }) => {
    expect(normalizeReceiptPayload(input)).toBeNull();
  });
});

describe("ActionReceipt model - cap lengths (60/60/90)", () => {
  const capCases = [
    {
      name: "headline capped to 60 characters",
      input: {
        kind: "open_app",
        headline: "A".repeat(80),
        subject: "App",
        ok: true,
      },
      check: (res: ActionReceipt | null) => {
        expect(res).not.toBeNull();
        expect(res!.headline).toBe("A".repeat(60));
        expect(res!.headline.length).toBe(60);
      },
    },
    {
      name: "subject capped to 60 characters",
      input: {
        kind: "open_site",
        headline: "Opened",
        subject: "B".repeat(100),
        ok: true,
      },
      check: (res: ActionReceipt | null) => {
        expect(res).not.toBeNull();
        expect(res!.subject).toBe("B".repeat(60));
        expect(res!.subject.length).toBe(60);
      },
    },
    {
      name: "title capped to 60 characters",
      input: {
        kind: "search",
        headline: "Opened",
        subject: "Google",
        title: "T".repeat(75),
        ok: true,
      },
      check: (res: ActionReceipt | null) => {
        expect(res).not.toBeNull();
        expect(res!.title).toBe("T".repeat(60));
        expect(res!.title!.length).toBe(60);
      },
    },
    {
      name: "detail capped to 90 characters",
      input: {
        kind: "search",
        headline: "Opened",
        subject: "youtube.com",
        detail: "D".repeat(120),
        ok: true,
      },
      check: (res: ActionReceipt | null) => {
        expect(res).not.toBeNull();
        expect(res!.detail).toBe("D".repeat(90));
        expect(res!.detail!.length).toBe(90);
      },
    },
  ];

  it.each(capCases)("$name", ({ input, check }) => {
    check(normalizeReceiptPayload(input));
  });
});

describe("ActionReceipt model - visible and expiry table", () => {
  const t0 = 100_000;
  const initialReceipt: ActionReceipt = {
    kind: "open_app",
    headline: "Opened",
    subject: "Notepad",
    title: null,
    detail: null,
    ok: true,
  };

  const stateActive: ActionReceiptState = {
    receipt: initialReceipt,
    visible: true,
    timestamp: t0,
  };

  const expiryTable = [
    { offsetMs: 0, expectedExpired: false, expectedShouldShow: true },
    { offsetMs: 1000, expectedExpired: false, expectedShouldShow: true },
    { offsetMs: 5000, expectedExpired: false, expectedShouldShow: true },
    { offsetMs: 5999, expectedExpired: false, expectedShouldShow: true },
    { offsetMs: 6000, expectedExpired: true, expectedShouldShow: false },
    { offsetMs: 6500, expectedExpired: true, expectedShouldShow: false },
    { offsetMs: 10000, expectedExpired: true, expectedShouldShow: false },
  ];

  it.each(expiryTable)(
    "at offset +$offsetMs ms -> expired: $expectedExpired, shouldShow: $expectedShouldShow",
    ({ offsetMs, expectedExpired, expectedShouldShow }) => {
      const now = t0 + offsetMs;
      expect(isReceiptExpired(stateActive, now)).toBe(expectedExpired);
      expect(shouldShowReceipt(stateActive, now)).toBe(expectedShouldShow);
    },
  );

  it("handleTick auto-hides once 6 s has elapsed", () => {
    expect(handleTick(stateActive, t0 + 5999).visible).toBe(true);
    const expiredState = handleTick(stateActive, t0 + RECEIPT_AUTO_HIDE_MS);
    expect(expiredState.visible).toBe(false);
    expect(expiredState.timestamp).toBeNull();
  });

  it("returns false for expired if not visible or timestamp is null", () => {
    expect(isReceiptExpired(INITIAL_ACTION_RECEIPT_STATE, t0 + 10000)).toBe(false);
    expect(shouldShowReceipt(INITIAL_ACTION_RECEIPT_STATE, t0)).toBe(false);
  });
});

describe("ActionReceipt model - replacement table", () => {
  const t0 = 10_000;
  const receiptA = {
    kind: "open_app" as const,
    headline: "Opened",
    subject: "Notepad",
    title: null,
    detail: null,
    ok: true,
  };
  const receiptB = {
    kind: "open_site" as const,
    headline: "Opened",
    subject: "youtube.com",
    title: "lofi",
    detail: null,
    ok: true,
  };

  const replacementCases = [
    {
      name: "valid receipt B replaces receipt A at t=3000 and restarts 6 s timer",
      initial: handleReceipt(INITIAL_ACTION_RECEIPT_STATE, receiptA, t0),
      incoming: receiptB,
      arrivalMs: t0 + 3000,
      expectedHeadline: "Opened - youtube.com",
      expectedTimestamp: t0 + 3000,
      testNowMs: t0 + 7000, // at t0 + 7000, receipt A would have expired, but receipt B is still active
      expectedVisibleAtTestNow: true,
    },
    {
      name: "malformed incoming payload does not replace receipt A and preserves timer",
      initial: handleReceipt(INITIAL_ACTION_RECEIPT_STATE, receiptA, t0),
      incoming: { malformed: true },
      arrivalMs: t0 + 3000,
      expectedHeadline: "Opened - Notepad",
      expectedTimestamp: t0,
      testNowMs: t0 + 6500, // at t0 + 6500, original receipt A has expired
      expectedVisibleAtTestNow: false,
    },
  ];

  it.each(replacementCases)(
    "$name",
    ({ initial, incoming, arrivalMs, expectedHeadline, expectedTimestamp, testNowMs, expectedVisibleAtTestNow }) => {
      const state = handleReceipt(initial, incoming, arrivalMs);
      expect(formatReceiptHeadline(state.receipt!)).toBe(expectedHeadline);
      expect(state.timestamp).toBe(expectedTimestamp);
      expect(shouldShowReceipt(state, testNowMs)).toBe(expectedVisibleAtTestNow);
    },
  );
});

describe("ActionReceipt model - cancellation", () => {
  it("hides active receipt on cancel", () => {
    const active = handleReceipt(INITIAL_ACTION_RECEIPT_STATE, {
      kind: "open_app",
      headline: "Opened",
      subject: "Notepad",
      ok: true,
    }, 5000);

    expect(active.visible).toBe(true);
    const canceled = handleCancel(active);
    expect(canceled.visible).toBe(false);
    expect(canceled.timestamp).toBeNull();
    expect(shouldShowReceipt(canceled, 5500)).toBe(false);
  });
});

describe("ActionReceiptModel class with injected clock", () => {
  it("manages complete lifecycle with clock progression", () => {
    let currentClock = 1000;
    const model = new ActionReceiptModel(() => currentClock);

    expect(model.isVisible()).toBe(false);
    expect(model.getReceipt()).toBeNull();

    // Trigger receipt
    model.onReceipt({
      kind: "search",
      headline: "Opened",
      subject: "youtube.com",
      title: "lofi beats",
      detail: "Search results. Nothing is playing yet.",
      ok: true,
    });

    expect(model.isVisible()).toBe(true);
    expect(model.getReceipt()?.subject).toBe("youtube.com");

    // Advance 4 s
    currentClock += 4000;
    model.tick();
    expect(model.isVisible()).toBe(true);

    // Advance past 6 s total
    currentClock += 2001;
    model.tick();
    expect(model.isVisible()).toBe(false);

    // Re-trigger and cancel
    model.onReceipt({
      kind: "open_app",
      headline: "Tried to open",
      subject: "Calculator",
      ok: false,
    });
    expect(model.isVisible()).toBe(true);
    model.onCancel();
    expect(model.isVisible()).toBe(false);
  });
});
