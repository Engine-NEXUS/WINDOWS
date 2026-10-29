import { describe, expect, it } from "vitest";
import { decideSpeech, speechKindForResult } from "./speechArbiter";

describe("main command center speech arbitration", () => {
  it("lets a result supersede its acknowledgement", () => {
    expect(decideSpeech({ kind: "ack", requestId: "r1" }, { kind: "result", requestId: "r1" })).toBe("play");
  });

  it("does not let a late acknowledgement interrupt a result", () => {
    expect(decideSpeech({ kind: "result", requestId: "r1" }, { kind: "ack", requestId: "r1" })).toBe("drop");
  });

  it("queues an alert while a result speaks, but a new result supersedes it", () => {
    expect(decideSpeech({ kind: "result", requestId: "r1" }, { kind: "alert", requestId: "a1" })).toBe("queue");
    expect(decideSpeech({ kind: "result", requestId: "r1" }, { kind: "result", requestId: "r2" })).toBe("play");
  });

  it("holds a same-turn result behind a confirmation prompt", () => {
    expect(decideSpeech({ kind: "prompt", requestId: "r1" }, { kind: "result", requestId: "r1" })).toBe("queue");
  });

  it("treats background speech as an alert so it waits for a playing answer", () => {
    for (const id of ["sentinel_boot_hello_boot_1", "sentinel_nb_github:zync:pr_9", "sentinel_offer_study", "boot_briefing"]) {
      expect(speechKindForResult(id)).toBe("alert");
      expect(decideSpeech({ kind: "result", requestId: "r1" }, { kind: speechKindForResult(id), requestId: id })).toBe("queue");
    }
  });

  it("keeps an answer to the user a result, which still supersedes", () => {
    for (const id of ["e2298774643c", "adf342d98b46", undefined]) {
      expect(speechKindForResult(id)).toBe("result");
    }
    expect(decideSpeech({ kind: "result", requestId: "r1" }, { kind: speechKindForResult("r2"), requestId: "r2" })).toBe("play");
  });

  it("applies the same priority to a newly added subcenter", () => {
    expect(decideSpeech(
      { kind: "result", requestId: "r1", sourceCenter: "main" },
      { kind: "alert", requestId: "x1", sourceCenter: "test_subcenter" },
    )).toBe("queue");
  });
});
