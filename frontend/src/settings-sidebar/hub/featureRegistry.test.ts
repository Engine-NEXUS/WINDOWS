import { describe, expect, it } from "vitest";
import {
  FEATURES,
  blockedBy,
  defaultsPatch,
  effectiveValue,
  isOn,
  matches,
  patchFor,
  safeModePatch,
  type Feature,
} from "./featureRegistry";

const find = (key: string): Feature => {
  const f = FEATURES.find((x) => x.key === key);
  if (!f) throw new Error(`no feature with key ${key}`);
  return f;
};

describe("registry shape", () => {
  it("has unique ids and unique setting keys", () => {
    expect(new Set(FEATURES.map((f) => f.id)).size).toBe(FEATURES.length);
    expect(new Set(FEATURES.map((f) => f.key)).size).toBe(FEATURES.length);
  });

  it("every dependsOn names a real key, and never itself", () => {
    const keys = new Set(FEATURES.map((f) => f.key));
    for (const f of FEATURES) {
      if (!f.dependsOn) continue;
      expect(keys.has(f.dependsOn), `${f.id} depends on ${f.dependsOn}`).toBe(true);
      expect(f.dependsOn).not.toBe(f.key);
    }
  });

  it("enum rows have options and a default among them; bool rows have a boolean default", () => {
    for (const f of FEATURES) {
      if (f.kind === "enum") {
        expect(f.options && f.options.length > 1, f.id).toBe(true);
        expect(f.options).toContain(f.def);
      } else {
        expect(typeof f.def, f.id).toBe("boolean");
      }
    }
  });

  it("no row claims a readiness it was not given", () => {
    expect(FEATURES.filter((f) => f.readiness !== undefined)).toEqual([]);
  });
});

describe("effective value", () => {
  const f = find("memcoreActivity");
  it("uses the stored boolean, else the default", () => {
    expect(effectiveValue({ memcoreActivity: false }, f)).toBe(false);
    expect(effectiveValue({}, f)).toBe(true);
    expect(effectiveValue({ memcoreActivity: "yes" }, f)).toBe(true); // wrong type is ignored
  });

  it("an enum value outside its options falls back to the default", () => {
    const w = find("watchPower");
    expect(effectiveValue({ watchPower: "on" }, w)).toBe("on");
    expect(effectiveValue({ watchPower: "banana" }, w)).toBe("off");
    expect(isOn({ watchPower: "dry" }, w)).toBe(true);
    expect(isOn({ watchPower: "off" }, w)).toBe(false);
  });
});

describe("dependencies", () => {
  it("a child is blocked while its parent is off, not otherwise", () => {
    const child = find("handSnap");
    expect(blockedBy({ handMode: false }, child)?.key).toBe("handMode");
    expect(blockedBy({ handMode: true }, child)).toBeNull();
    expect(blockedBy({}, find("handMode"))).toBeNull();
  });

  it("blocking follows a chain one step at a time", () => {
    const grandchild = find("handFocusLowerApps");
    expect(blockedBy({ handMode: true, handFocusMode: false }, grandchild)?.key).toBe("handFocusMode");
  });
});

describe("a switch writes only its own key", () => {
  it("patchFor carries exactly one key", () => {
    expect(patchFor(find("screenTour"), false)).toEqual({ screenTour: false });
    expect(patchFor(find("watchPower"), "dry")).toEqual({ watchPower: "dry" });
  });
});

describe("safe mode", () => {
  const allOn = Object.fromEntries(FEATURES.map((f) => [f.key, f.kind === "bool" ? true : f.options![2] ?? f.options![1]]));

  it("turns off risky features that are on, and nothing else", () => {
    const patch = safeModePatch(allOn);
    for (const f of FEATURES) {
      const expected = f.risk !== "safe" && !f.keepInSafeMode;
      expect(f.key in patch, f.id).toBe(expected);
    }
  });

  it("never touches the protections or the way back", () => {
    const patch = safeModePatch(allOn);
    for (const key of ["speakerVerification", "localSttOnly", "verifyWake", "autostart"]) expect(key in patch, key).toBe(false);
  });

  it("switches an enum to its off option", () => {
    expect(safeModePatch({ watchPower: "on" }).watchPower).toBe("off");
  });

  it("does nothing when everything risky is already off", () => {
    const quiet = Object.fromEntries(FEATURES.map((f) => [f.key, f.kind === "bool" ? false : f.options![0]]));
    expect(safeModePatch(quiet)).toEqual({});
  });
});

describe("restore defaults", () => {
  it("restores every default except the protective ones", () => {
    const patch = defaultsPatch();
    expect(patch.screenTour).toBe(true);
    expect(patch.watchPower).toBe("off");
    for (const key of ["speakerVerification", "localSttOnly", "verifyWake"]) expect(key in patch, key).toBe(false);
  });
});

describe("filters", () => {
  const s = { screenTour: false, memcoreMail: true };
  it("on / off / needs-care / search", () => {
    expect(matches(s, find("screenTour"), "off", "")).toBe(true);
    expect(matches(s, find("screenTour"), "on", "")).toBe(false);
    expect(matches(s, find("memcoreMail"), "on", "")).toBe(true);
    expect(matches(s, find("memcoreMail"), "risky", "")).toBe(false); // mail watcher is safe
    expect(matches(s, find("screenTour"), "risky", "")).toBe(true); // sends a screenshot out
    expect(matches(s, find("screenTour"), "all", "TOUR")).toBe(true);
    expect(matches(s, find("screenTour"), "all", "nonsense-word")).toBe(false);
  });
});
