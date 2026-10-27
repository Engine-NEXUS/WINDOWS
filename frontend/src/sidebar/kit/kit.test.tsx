import { describe, it, expect, vi } from "vitest";
import { renderToString } from "react-dom/server";
import { LogoTile, hashHue } from "./LogoTile";
import { Donut } from "./Donut";
import { DropdownChip, Chip } from "./Chip";
import { MiniBar } from "./MiniBar";
import { BarChart } from "./BarChart";

describe("Kit Components", () => {
  describe("LogoTile", () => {
    it("renders image mode when valid src is provided", () => {
      const html = renderToString(
        <LogoTile src="https://example.com/logo.png" name="Next.js" size={36} />,
      );
      expect(html).toContain('data-mode="image"');
      expect(html).toContain('<img');
      expect(html).toContain('src="https://example.com/logo.png"');
      expect(html).toContain('alt="Next.js"');
      expect(html).toContain('width:36px;height:36px');
    });

    it("renders deterministic monogram fallback when src is missing", () => {
      const html = renderToString(
        <LogoTile src={null} name="Stripe" size={36} />,
      );
      expect(html).toContain('data-mode="monogram"');
      expect(html).toContain('data-monogram="S"');
      expect(html).toContain(">S</span>");

      const hue = hashHue("Stripe");
      expect(html).toContain(`hsl(${hue}, 65%, 40%)`);
    });

    it("computes deterministic hues for identical names", () => {
      const hue1 = hashHue("React");
      const hue2 = hashHue("React");
      const hue3 = hashHue("Tailwind");
      expect(hue1).toBe(hue2);
      expect(hue1).toBeGreaterThanOrEqual(0);
      expect(hue1).toBeLessThan(360);
      expect(hue1).not.toBe(hue3);
    });

    it("handles full vs lg border radius", () => {
      const htmlFull = renderToString(
        <LogoTile name="User" rounded="full" />,
      );
      expect(htmlFull).toContain("kit-logo-tile--full");

      const htmlLg = renderToString(
        <LogoTile name="App" rounded="lg" />,
      );
      expect(htmlLg).toContain("kit-logo-tile--lg");
    });
  });

  describe("Donut", () => {
    it("skips zero-value slices and ensures rendered slice angles sum to 360", () => {
      const slices = [
        { label: "Frontend", value: 4, colorIndex: 0 },
        { label: "EmptyCat", value: 0, colorIndex: 1 },
        { label: "Backend", value: 2, colorIndex: 2 },
        { label: "Analytics", value: 2, colorIndex: 3 },
        { label: "ZeroValue", value: 0, colorIndex: 4 },
      ];

      const html = renderToString(
        <Donut
          slices={slices}
          centerTop="8"
          centerBottom="technologies"
          size={140}
        />,
      );

      // Verify center labels
      expect(html).toContain(">8</text>");
      expect(html).toContain(">technologies</text>");

      // Verify zero-value slices are skipped
      expect(html).not.toContain('data-label="EmptyCat"');
      expect(html).not.toContain('data-label="ZeroValue"');

      // Verify non-zero slices are rendered
      expect(html).toContain('data-label="Frontend"');
      expect(html).toContain('data-label="Backend"');
      expect(html).toContain('data-label="Analytics"');

      // Extract all rendered data-angle values and verify they sum to exactly 360
      const matches = Array.from(html.matchAll(/data-angle="([0-9.]+)"/g));
      expect(matches.length).toBe(3);

      const angles = matches.map((m) => parseFloat(m[1]));
      const sum = angles.reduce((a, b) => a + b, 0);
      expect(Math.round(sum)).toBe(360);

      // Frontend: 4 / 8 = 180 degrees
      expect(angles[0]).toBeCloseTo(180, 5);
      // Backend: 2 / 8 = 90 degrees
      expect(angles[1]).toBeCloseTo(90, 5);
      // Analytics: 2 / 8 = 90 degrees
      expect(angles[2]).toBeCloseTo(90, 5);
    });

    it("renders empty circle when all slices are 0 or empty", () => {
      const html = renderToString(
        <Donut
          slices={[{ label: "None", value: 0, colorIndex: 0 }]}
          centerTop="0"
          centerBottom="items"
        />,
      );
      expect(html).not.toContain('data-slice="true"');
      expect(html).toContain(">0</text>");
      expect(html).toContain(">items</text>");
    });
  });

  describe("DropdownChip", () => {
    it("renders native select with options and handles value change", () => {
      const options = ["By category", "By confidence"];
      const onChange = vi.fn();

      const html = renderToString(
        <DropdownChip
          value="By category"
          options={options}
          onChange={onChange}
          ariaLabel="Select group mode"
        />,
      );

      expect(html).toContain("<select");
      expect(html).toContain('class="kit-dropdown-chip "');
      expect(html).toContain('aria-label="Select group mode"');
      expect(html).toContain('<option value="By category" selected="">By category</option>');
      expect(html).toContain('<option value="By confidence">By confidence</option>');
    });

    it("supports object options with label and value", () => {
      const options = [
        { label: "All Items", value: "all" },
        { label: "Confirmed Only", value: "confirmed" },
      ];
      const html = renderToString(
        <DropdownChip
          value="confirmed"
          options={options}
          onChange={() => {}}
        />,
      );
      expect(html).toContain('<option value="all">All Items</option>');
      expect(html).toContain('<option value="confirmed" selected="">Confirmed Only</option>');
    });
  });

  describe("Chip, MiniBar, and BarChart", () => {
    it("renders Chip with tones and dot", () => {
      const html = renderToString(
        <Chip tone="good" dot="#10b981">
          Confirmed
        </Chip>,
      );
      expect(html).toContain("kit-chip--good");
      expect(html).toContain('style="background-color:#10b981"');
      expect(html).toContain("Confirmed");
    });

    it("renders MiniBar with pass and fail states", () => {
      const passHtml = renderToString(<MiniBar label="HSTS" pass={true} detail="max-age" />);
      expect(passHtml).toContain("kit-minibar--pass");
      expect(passHtml).toContain("✓");
      expect(passHtml).toContain("HSTS");
      expect(passHtml).toContain("max-age");

      const failHtml = renderToString(<MiniBar label="CSP" pass={false} />);
      expect(failHtml).toContain("kit-minibar--fail");
      expect(failHtml).toContain("✗");
      expect(failHtml).toContain("CSP");
    });

    it("renders BarChart with formatted values and widths", () => {
      const bars = [
        { label: "JavaScript", value: 500, colorIndex: 0 },
        { label: "HTML", value: 250, colorIndex: 1 },
      ];
      const html = renderToString(
        <BarChart bars={bars} formatValue={(n) => `${n} KB`} />,
      );
      expect(html).toContain("JavaScript");
      expect(html).toContain("500 KB");
      expect(html).toContain("HTML");
      expect(html).toContain("250 KB");
      expect(html).toContain("width:100%");
      expect(html).toContain("width:50%");
    });
  });
});
