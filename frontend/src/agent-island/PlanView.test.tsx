import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PlanView } from "./AgentIslandApp";

const PLAN = "# Plan\n\n1. Edit **monitor.rs**\n2. Add `tests`\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n```rust\nlet x = 1;\n```\n\n- [x] done\n";

describe("the plan card draws real text", () => {
  const html = renderToStaticMarkup(<PlanView text={PLAN} />);

  it("uses headings, lists, bold, code, a table and a code block", () => {
    expect(html).toContain("<h1>Plan</h1>");
    expect(html).toContain("<ol");
    expect(html).toContain("<strong>monitor.rs</strong>");
    expect(html).toContain("<code>tests</code>");
    expect(html).toContain("<table>");
    expect(html).toContain("<pre class=\"ai-plan-code\"><code>let x = 1;</code></pre>");
    expect(html).toContain("ai-box--on");
  });

  it("shows none of the markdown symbols as text", () => {
    const text = html.replace(/<[^>]*>/g, "");
    expect(text).not.toMatch(/[#*`|]/);
    expect(text).not.toContain("---");
  });

  it("escapes anything that looks like markup", () => {
    const evil = renderToStaticMarkup(<PlanView text={'<img src=x onerror="alert(1)"> <script>alert(1)</script> [go](javascript:alert(1))'} />);
    expect(evil).not.toContain("<script");
    expect(evil).not.toContain("<img");
    expect(evil).not.toContain("onerror");
    expect(evil).not.toContain("javascript:");
    expect(evil).not.toContain("href");
  });

  it("says so when there is no plan text", () => {
    expect(renderToStaticMarkup(<PlanView text="" />)).toContain("(no details)");
  });
});
