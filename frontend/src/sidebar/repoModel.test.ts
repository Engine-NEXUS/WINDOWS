import { describe, expect, it } from "vitest";
import { compactCount, languageRows } from "./repoModel";

describe("repository view model", () => {
  it("keeps the five largest languages and folds the rest into Other", () => {
    const rows = languageRows([
      { name: "A", bytes: 1, percentage: 40 },
      { name: "B", bytes: 1, percentage: 25 },
      { name: "C", bytes: 1, percentage: 15 },
      { name: "D", bytes: 1, percentage: 8 },
      { name: "E", bytes: 1, percentage: 5 },
      { name: "F", bytes: 1, percentage: 4 },
      { name: "G", bytes: 1, percentage: 3 },
    ]);
    expect(rows.map((r) => r.name)).toEqual(["A", "B", "C", "D", "E", "Other"]);
    expect(rows[5].percent).toBe(7);
  });

  it("sorts by share, drops empty and invalid entries and tones the bars ink, mid, low", () => {
    const rows = languageRows([
      { name: "Small", bytes: 1, percentage: 10 },
      { name: "Zero", bytes: 0, percentage: 0 },
      { name: "Big", bytes: 1, percentage: 90 },
      { name: "Bad", bytes: 1, percentage: Number.NaN },
    ]);
    expect(rows.map((r) => r.name)).toEqual(["Big", "Small"]);
    expect(rows.map((r) => r.tone)).toEqual(["ink", "mid"]);
    expect(languageRows([{ name: "A", bytes: 1, percentage: 50 }, { name: "B", bytes: 1, percentage: 30 }, { name: "C", bytes: 1, percentage: 20 }])[2].tone).toBe("low");
  });

  it("clamps the percentages and copes with a missing list", () => {
    expect(languageRows([{ name: "A", bytes: 1, percentage: 140 }])[0].percent).toBe(100);
    expect(languageRows(undefined)).toEqual([]);
    expect(languageRows(null)).toEqual([]);
  });

  it("does not add an Other bar for a rounding sliver", () => {
    const six = ["A", "B", "C", "D", "E"].map((name) => ({ name, bytes: 1, percentage: 19 }));
    expect(languageRows([...six, { name: "F", bytes: 1, percentage: 0.2 }]).some((r) => r.name === "Other")).toBe(false);
  });

  it("writes counts compactly", () => {
    expect(compactCount(950)).toBe("950");
    expect(compactCount(1200)).toBe("1.2k");
    expect(compactCount(2000)).toBe("2k");
    expect(compactCount(12_400)).toBe("12k");
    expect(compactCount(2_500_000)).toBe("2.5M");
    expect(compactCount(undefined)).toBe("–");
    expect(compactCount(-3)).toBe("–");
  });
});
