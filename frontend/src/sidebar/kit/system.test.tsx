import { describe, expect, it } from "vitest";
import { renderToString } from "react-dom/server";
import {
  ActionBar,
  BarRow,
  Callout,
  KpiTile,
  NumberedList,
  Pill,
  SegPill,
  SeverityPill,
  SvBar,
  SvButton,
  SvView,
  VoiceTimer,
  calloutKindOf,
  clampPercent,
  severityOf,
} from "./system";

describe("sidebar system kit", () => {
  it("the header has no dock arrows and no close button (arrows dock the window, Esc closes it)", () => {
    const html = renderToString(<SvBar icon="monitor" title="Screen analysis" subtitle="Brave" />);
    expect(html).toContain("Screen analysis");
    expect(html).toContain("Brave");
    expect(html).not.toContain("<button");
    expect(html).not.toMatch(/dock|close/i);
  });

  it("a view carries the .sv token scope", () => {
    expect(renderToString(<SvView>x</SvView>)).toContain('class="sv sv-view"');
  });

  it("severity is a shape and a word, never colour alone", () => {
    expect(severityOf("high")).toMatchObject({ tone: "solid", symbol: "▲", word: "High" });
    expect(severityOf("Critical").word).toBe("High");
    expect(severityOf("medium")).toMatchObject({ tone: "outline", symbol: "◆", word: "Medium" });
    expect(severityOf("warning").word).toBe("Medium");
    expect(severityOf("minor").word).toBe("Medium");
    expect(severityOf("trivial").word).toBe("Low");
    expect(severityOf("info").word).toBe("Low");
    expect(severityOf("low")).toMatchObject({ tone: "dash", symbol: "●", word: "Low" });
    expect(severityOf(undefined).word).toBe("Low");
    expect(severityOf("high").rank).toBeLessThan(severityOf("medium").rank);
    const html = renderToString(<SeverityPill severity="high" />);
    expect(html).toContain("▲");
    expect(html).toContain("High");
    expect(html).toContain("sv-pill--solid");
  });

  it("a bar value is clamped and shown on the right", () => {
    expect(clampPercent(250)).toBe(100);
    expect(clampPercent(-4)).toBe(0);
    expect(clampPercent(Number.NaN)).toBe(0);
    expect(clampPercent(41.6)).toBe(42);
    const html = renderToString(<BarRow label="Rust" value="48 %" percent={48} tone="mid" />);
    expect(html).toContain("Rust");
    expect(html).toContain("48 %");
    expect(html).toContain("width:48%");
    expect(html).toContain("sv-fill--mid");
  });

  it("a KPI tile shows a caps label, a big number and an optional sub line", () => {
    const html = renderToString(<KpiTile label="Files" value="6" sub="+ 2 new" />);
    expect(html).toContain("Files");
    expect(html).toContain('class="sv-kpi-v">6<');
    expect(html).toContain("+ 2 new");
    expect(renderToString(<KpiTile label="A" value="1" />)).not.toContain("sv-kpi-s");
  });

  it("a numbered list numbers its items from 1 and keeps the title and text", () => {
    const html = renderToString(<NumberedList items={[{ title: "Search box", text: "Type here" }, { title: "Shortcuts" }]} />);
    expect(html).toContain(">1<");
    expect(html).toContain(">2<");
    expect(html).toContain("Search box");
    expect(html).toContain("Type here");
  });

  it("a callout carries an icon and its word, so it never relies on colour", () => {
    expect(calloutKindOf("IMPORTANT")).toBe("important");
    expect(calloutKindOf("nonsense")).toBe("note");
    const important = renderToString(<Callout kind="important">Read this</Callout>);
    expect(important).toContain("Important");
    expect(important).toContain("<svg");
    expect(important).not.toContain("sv-callout--dim");
    expect(renderToString(<Callout kind="tip">Try this</Callout>)).toContain("sv-callout--dim");
  });

  it("buttons can show their key and a segmented filter marks the active item", () => {
    const html = renderToString(
      <ActionBar>
        <SvButton keyHint="Esc">Cancel</SvButton>
        <SvButton primary keyHint="Enter">Confirm</SvButton>
      </ActionBar>,
    );
    expect(html).toContain("Esc");
    expect(html).toContain("Enter");
    expect(html).toContain("sv-btn--primary");
    const seg = renderToString(<SegPill items={["All", "High"]} active={1} onChange={() => {}} />);
    expect(seg).toContain('aria-pressed="true"');
    expect(seg.match(/aria-pressed="true"/g)).toHaveLength(1);
  });

  it("the voice timer drains over the requested seconds (at least one)", () => {
    expect(renderToString(<VoiceTimer seconds={5} />)).toContain("animation-duration:5s");
    expect(renderToString(<VoiceTimer seconds={0} />)).toContain("animation-duration:1s");
  });

  it("pills render an icon when asked", () => {
    expect(renderToString(<Pill tone="solid" icon="check">Passed</Pill>)).toContain("<svg");
    expect(renderToString(<Pill>Plain</Pill>)).not.toContain("<svg");
  });
});
