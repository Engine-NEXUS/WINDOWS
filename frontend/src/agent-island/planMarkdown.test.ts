import { describe, expect, it } from "vitest";
import { planBlocks, plainText, type Block } from "./planMarkdown";

const PLAN = `# Plan: add the finished notice

## Steps
1. Edit \`agent_monitor.rs\` to keep the **finished** session
2. Add a *test* for it
   - covers the ~~old~~ new case
   - covers a failed turn

| File | Change |
|------|--------|
| a.rs | small |
| b.ts | large |

\`\`\`rust
fn main() {}
\`\`\`

> Risk: none

- [ ] build
- [x] test
`;

describe("the plan card reads like /plan", () => {
  const bs = planBlocks(PLAN);
  const kinds = bs.map((b) => b.t);

  it("keeps the structure: headings, ordered list, table, code, quote, task list", () => {
    expect(kinds).toEqual(["heading", "heading", "list", "table", "code", "quote", "list"]);
    expect(bs[0]).toMatchObject({ t: "heading", level: 1 });
    expect(bs[1]).toMatchObject({ t: "heading", level: 2 });
    expect(bs[2]).toMatchObject({ t: "list", ordered: true, start: 1 });
  });

  it("shows no markdown symbols in the words", () => {
    const text = plainText(bs);
    expect(text).toContain("Plan: add the finished notice");
    expect(text).toContain("Edit agent_monitor.rs to keep the finished session");
    expect(text).not.toMatch(/[#*`~|]/);
    expect(text).not.toContain("---");
  });

  it("reads inline bold, italics, code and strike-through as styles, not symbols", () => {
    const list = bs[2] as Extract<Block, { t: "list" }>;
    const first = list.items[0].blocks[0] as Extract<Block, { t: "paragraph" }>;
    expect(first.inlines.map((i) => i.t)).toEqual(["text", "code", "text", "strong", "text"]);
  });

  it("keeps the table cells and the code as they are", () => {
    const t = bs[3] as Extract<Block, { t: "table" }>;
    expect(t.head).toHaveLength(2);
    expect(t.rows).toHaveLength(2);
    expect(bs[4]).toMatchObject({ t: "code", lang: "rust", text: "fn main() {}" });
  });

  it("marks tasks done and not done", () => {
    const l = bs[6] as Extract<Block, { t: "list" }>;
    expect(l.items.map((i) => i.checked)).toEqual([false, true]);
  });
});

describe("a plan can never put markup on the page", () => {
  it("drops script tags, raw html, links and images down to their text", () => {
    const bs = planBlocks("Hello <script>alert(1)</script> [site](javascript:alert(1)) ![pic](http://x/y.png)\n\n<div onclick=\"x()\">block</div>");
    const text = plainText(bs);
    expect(text).not.toContain("<");
    expect(text).toContain("site");
    expect(text).toContain("pic");
    expect(JSON.stringify(bs)).not.toContain("javascript:");
    expect(JSON.stringify(bs)).not.toContain("onclick");
  });

  it("copes with empty, huge and deeply nested input without throwing", () => {
    expect(planBlocks("")).toEqual([]);
    expect(planBlocks("x".repeat(100_000)).length).toBeLessThanOrEqual(1);
    const nested = Array.from({ length: 40 }, (_, i) => `${"  ".repeat(i)}- item ${i}`).join("\n");
    expect(() => planBlocks(nested)).not.toThrow();
    const many = Array.from({ length: 3_000 }, (_, i) => `# h${i}`).join("\n\n");
    expect(planBlocks(many).length).toBeLessThanOrEqual(400);
  });
});
