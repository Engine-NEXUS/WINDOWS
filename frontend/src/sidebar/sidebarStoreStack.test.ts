import { describe, it, expect, beforeEach } from "vitest";
import { useSidebar } from "./sidebarStore";
import { SAMPLE_PAYLOAD } from "./devStack";

describe("sidebarStore stack actions", () => {
  beforeEach(() => {
    useSidebar.getState().hide();
  });

  it("showStack sets stackData, visible, query, and resets other view state", () => {
    useSidebar.getState().showStack(SAMPLE_PAYLOAD);

    const state = useSidebar.getState();
    expect(state.visible).toBe(true);
    expect(state.query).toBe(SAMPLE_PAYLOAD.title);
    expect(state.stackData).toEqual(SAMPLE_PAYLOAD);
    expect(state.spatialData).toBeNull();
    expect(state.analysisData).toBeNull();
    expect(state.codeRabbitData).toBeNull();
  });

  it("setStackProgress updates progress state", () => {
    const progress = {
      phase: "scanning" as const,
      text: "Parsing CSS bundles...",
      step: 2,
      total: 5,
    };
    useSidebar.getState().setStackProgress(progress);
    expect(useSidebar.getState().stackProgress).toEqual(progress);

    useSidebar.getState().setStackProgress(null);
    expect(useSidebar.getState().stackProgress).toBeNull();
  });

  it("setStackFocus sets and clears focus ids array safely", () => {
    useSidebar.getState().setStackFocus(["nextjs", "tailwind"]);
    expect(useSidebar.getState().stackFocus).toEqual(["nextjs", "tailwind"]);

    useSidebar.getState().setStackFocus([]);
    expect(useSidebar.getState().stackFocus).toEqual([]);

    // handles null/undefined safely
    useSidebar.getState().setStackFocus(null as any);
    expect(useSidebar.getState().stackFocus).toEqual([]);
  });

  it("hide resets stackData, stackProgress, and stackFocus", () => {
    useSidebar.getState().showStack(SAMPLE_PAYLOAD);
    useSidebar.getState().setStackProgress({ phase: "done", text: "done" });
    useSidebar.getState().setStackFocus(["nextjs"]);

    useSidebar.getState().hide();
    const state = useSidebar.getState();
    expect(state.visible).toBe(false);
    expect(state.stackData).toBeNull();
    expect(state.stackProgress).toBeNull();
    expect(state.stackFocus).toEqual([]);
  });

  it("other show actions clear stackData", () => {
    useSidebar.getState().showStack(SAMPLE_PAYLOAD);
    expect(useSidebar.getState().stackData).not.toBeNull();

    useSidebar.getState().show("General Query", "Response text");
    expect(useSidebar.getState().stackData).toBeNull();
  });

  it("mergeStackLogos adds logos to matching detections and keeps them on later payloads", () => {
    const first = SAMPLE_PAYLOAD.detections[0];
    useSidebar.getState().showStack({ ...SAMPLE_PAYLOAD, scanning: false, detections: SAMPLE_PAYLOAD.detections.map((d) => ({ ...d, logo: null })) });
    useSidebar.getState().mergeStackLogos({ site_logo: "data:image/png;base64,SITE", logos: { [first.id]: "data:image/png;base64,AAAA", unknown: "data:image/png;base64,ZZZZ" } });

    let data = useSidebar.getState().stackData!;
    expect(data.site_logo).toBe("data:image/png;base64,SITE");
    expect(data.detections.find((d) => d.id === first.id)?.logo).toBe("data:image/png;base64,AAAA");
    expect(data.detections.filter((d) => d.logo).length).toBe(1);

    // a later full payload of the same scan (sections added) keeps the logos
    useSidebar.getState().showStack({ ...SAMPLE_PAYLOAD, scanning: false, detections: SAMPLE_PAYLOAD.detections.map((d) => ({ ...d, logo: null })) });
    data = useSidebar.getState().stackData!;
    expect(data.detections.find((d) => d.id === first.id)?.logo).toBe("data:image/png;base64,AAAA");
    expect(data.site_logo).toBe("data:image/png;base64,SITE");

    // a NEW scan starts clean
    useSidebar.getState().showStack({ ...SAMPLE_PAYLOAD, scanning: true });
    expect(useSidebar.getState().stackLogoPatch).toBeNull();
  });
});
