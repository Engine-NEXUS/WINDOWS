import { describe, expect, it } from "vitest";
import { DIFF_MAX_BYTES, diffOf, fileName, type EditView } from "./diff";
import { languageOf, tokenize } from "./syntax";

const edit = (oldText: string, newText: string, path = "src/invoice.ts"): EditView => ({
  path,
  kind: "edit",
  pairs: [{ old: oldText, new: newText }],
  tooLarge: false,
});

describe("diffOf", () => {
  it("shows a changed line as one removal and one addition with context", () => {
    const old = "import { Item } from './types'\n\nconst TVA = 0.196\n\nexport function total() {}";
    const next = "import { Item } from './types'\n\nconst TVA = 0.2\n\nexport function total() {}";
    const d = diffOf(edit(old, next))!;
    expect(d.added).toBe(1);
    expect(d.removed).toBe(1);
    const lines = d.hunks.flatMap((h) => h.lines);
    expect(lines.filter((l) => l.kind === "removed").map((l) => l.text)).toEqual(["const TVA = 0.196"]);
    expect(lines.filter((l) => l.kind === "added").map((l) => l.text)).toEqual(["const TVA = 0.2"]);
    expect(lines.some((l) => l.kind === "context")).toBe(true);
  });

  it("numbers the lines of both sides", () => {
    const d = diffOf(edit("a\nb\nc", "a\nB\nc"))!;
    const removed = d.hunks[0].lines.find((l) => l.kind === "removed")!;
    const added = d.hunks[0].lines.find((l) => l.kind === "added")!;
    expect(removed.origLine).toBe(2);
    expect(added.newLine).toBe(2);
  });

  it("treats a Write as an all-added new file", () => {
    const d = diffOf({ path: "a.txt", kind: "write", pairs: [{ old: "", new: "one\ntwo\n" }], tooLarge: false })!;
    expect(d.isNewFile).toBe(true);
    expect(d.added).toBe(2);
    expect(d.removed).toBe(0);
  });

  it("merges the hunks of a MultiEdit", () => {
    const d = diffOf({
      path: "a.rs",
      kind: "multi",
      pairs: [
        { old: "x", new: "y" },
        { old: "p", new: "q" },
      ],
      tooLarge: false,
    })!;
    expect(d.added).toBe(2);
    expect(d.removed).toBe(2);
    expect(d.hunks).toHaveLength(2);
  });

  it("returns null for a change that changes nothing", () => {
    expect(diffOf(edit("same", "same"))).toBeNull();
    expect(diffOf(edit("", ""))).toBeNull();
    expect(diffOf({ path: "a", kind: "write", pairs: [{ old: "", new: "" }], tooLarge: false })).toBeNull();
  });

  it("keeps only a flag for a diff Rust or the guards say is too large", () => {
    expect(diffOf({ path: "big", kind: "write", pairs: [], tooLarge: true })!.tooLarge).toBe(true);
    const big = "line\n".repeat(DIFF_MAX_BYTES / 5 + 10);
    const d = diffOf(edit(big, big + "extra\n"))!;
    expect(d.tooLarge).toBe(true);
    expect(d.hunks).toEqual([]);
  });
});

describe("fileName", () => {
  it("reads both separators", () => {
    expect(fileName("D:\\w\\src\\invoice.ts")).toBe("invoice.ts");
    expect(fileName("/home/me/a.rs")).toBe("a.rs");
    expect(fileName("plain.md")).toBe("plain.md");
  });
});

describe("syntax", () => {
  it("names the language on the tab", () => {
    expect(languageOf("src/invoice.ts").label).toBe("TS");
    expect(languageOf("a.rs").label).toBe("RS");
    expect(languageOf("Makefile").label).toBe("TXT");
    expect(languageOf("x.weirdext").label).toBe("WEIR");
  });

  it("classifies keywords, types, calls, numbers, strings and comments", () => {
    const lang = languageOf("a.ts");
    const kinds = (line: string) => tokenize(line, lang).filter((t) => t.cls !== "plain").map((t) => `${t.cls}:${t.text}`);
    expect(kinds("const TVA = 0.196")).toEqual(["kw:const", "type:TVA", "num:0.196"]);
    expect(kinds("export function total(items: Item[]) {")).toEqual(["kw:export", "kw:function", "fn:total", "type:Item"]);
    expect(kinds("import { Item } from './types'")).toEqual(["kw:import", "type:Item", "kw:from", "str:'./types'"]);
    expect(kinds("x = 1 // note")).toEqual(["num:1", "com:// note"]);
  });

  it("never loses or duplicates text", () => {
    const lang = languageOf("a.ts");
    for (const line of ["const a = 'unterminated", "  return sum * (1 + TVA)", "a // b 'c", "", "`tpl ${x}`", "\\"]) {
      expect(tokenize(line, lang).map((t) => t.text).join("")).toBe(line);
    }
  });
});
