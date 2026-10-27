import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Rect, Size } from "../tourGeometry";
import { Annotation } from "./Annotation";
import { DrawStyle } from "./drawKit";

describe("Annotation component rendering", () => {
  const viewport: Size = { w: 1920, h: 1080 };
  const target: Rect = { x: 200, y: 150, w: 300, h: 120 };
  const link: Rect = { x: 700, y: 400, w: 150, h: 80 };

  const renderAnn = (drawStyle: DrawStyle, t: Rect | null = target, opts?: { thumb?: string | null; link?: Rect | null; idx?: number }) => {
    const props = {
      style: drawStyle,
      target: t,
      link: opts?.link ?? null,
      thumb: opts?.thumb ?? null,
      viewport,
      live: true,
      idx: opts?.idx ?? 0,
    };
    return renderToString(
      <svg>
        <Annotation {...props} />
      </svg>
    );
  };

  it("renders empty group when target is null", () => {
    const html = renderAnn("ring", null);
    expect(html).toContain('class="ann-group"');
    expect(html).not.toContain("ann--ring");
  });

  it("renders ring style", () => {
    const html = renderAnn("ring");
    expect(html).toContain("ann--ring");
    expect(html).toContain("tour-ring");
  });

  it("renders underline style with wavy path and glow", () => {
    const html = renderAnn("underline");
    expect(html).toContain("ann--underline");
    expect(html).toContain("ann--underline-glow");
  });

  it("renders bracket style with left and right paths", () => {
    const html = renderAnn("bracket");
    expect(html).toContain("ann--bracket");
    expect(html).toContain("ann--bracket-left");
    expect(html).toContain("ann--bracket-right");
  });

  it("renders circle style ellipse", () => {
    const html = renderAnn("circle");
    expect(html).toContain("ann--circle");
    expect(html).toContain("<ellipse");
  });

  it("renders arrow style with stem, head, and dot", () => {
    const html = renderAnn("arrow");
    expect(html).toContain("ann--arrow-stem");
    expect(html).toContain("ann--arrow-head");
    expect(html).toContain("ann--arrow-dot");
  });

  it("renders highlight style rectangle", () => {
    const html = renderAnn("highlight");
    expect(html).toContain("ann--highlight");
  });

  it("renders spotlight style path and spotlight ring", () => {
    const html = renderAnn("spotlight");
    expect(html).toContain("ann--spotlight");
    expect(html).toContain("ann--spotlight-ring");
    expect(html).toContain('fill-rule="evenodd"');
  });

  it("renders scan style box, clip path, and scan line", () => {
    const html = renderAnn("scan");
    expect(html).toContain("ann--scan-box");
    expect(html).toContain("ann--scan-line");
    expect(html).toContain("clipPath");
  });

  it("renders lens style bubble with connector, rim, beam, and image", () => {
    const html = renderAnn("lens", target, { thumb: "data:image/jpeg;base64,abc" });
    expect(html).toContain("ann--lens-connector");
    expect(html).toContain("ann--lens-rim-under");
    expect(html).toContain("ann--lens-rim");
    expect(html).toContain("ann--lens-beam");
    expect(html).toContain("<image");
    expect(html).toContain('href="data:image/jpeg;base64,abc"');
  });

  it("renders connect style with connecting line, beam, and boxes", () => {
    const html = renderAnn("connect", target, { link });
    expect(html).toContain("ann--connect-box");
    expect(html).toContain("ann--connect-line");
    expect(html).toContain("ann--connect-beam");
    expect(html).toContain("ann--connect-dot");
  });

  it("renders pin style with index number badge", () => {
    const html = renderAnn("pin", target, { idx: 2 });
    expect(html).toContain("ann--pin");
    expect(html).toContain("ann-pin-shape");
    expect(html).toContain(">3<"); // idx + 1 = 2 + 1 = 3
  });

  it("renders pointer trail cursor marker with outer ring and inner dot", () => {
    const html = renderAnn("ring");
    expect(html).toContain("ann-pointer");
    expect(html).toContain("ann-pointer-ring");
    expect(html).toContain("ann-pointer-dot");
  });
});
