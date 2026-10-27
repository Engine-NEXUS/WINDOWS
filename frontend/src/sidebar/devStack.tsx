import { StrictMode, useState, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { StackView } from "./StackView";
import { useSidebar } from "./sidebarStore";
import type { StackPayload, StackProgress } from "./stackTypes";
import "./sidebar.css";
import "./unified.css";
import "./stack.css";

// ── Minimal mock of the Tauri JS bridge for dev preview ────────────
let callbackId = 0;
const handlers: Record<string, number[]> = {};

/* eslint-disable @typescript-eslint/no-explicit-any */
if (typeof window !== "undefined") {
  (window as any).__TAURI_INTERNALS__ = {
    transformCallback(cb: (data: unknown) => void) {
      const id = ++callbackId;
      (window as any)[`_${id}`] = cb;
      return id;
    },
    invoke(cmd: string, args?: Record<string, unknown>) {
      if (cmd === "plugin:event|listen") {
        const event = args?.event as string;
        const handler = args?.handler as number;
        if (!handlers[event]) handlers[event] = [];
        handlers[event].push(handler);
        return Promise.resolve(handler);
      }
      if (cmd === "plugin:event|unlisten") {
        const event = args?.event as string;
        const eventId = args?.eventId as number;
        if (handlers[event]) {
          handlers[event] = handlers[event].filter((h) => h !== eventId);
        }
        return Promise.resolve();
      }
      if (cmd === "get_pending_stack") {
        return Promise.resolve(SAMPLE_PAYLOAD);
      }
      return Promise.resolve(null);
    },
    convertFileSrc: (s: string) => s,
  };

  (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: () => {},
  };
}

export function emit(event: string, payload: unknown): void {
  if (typeof window === "undefined") return;
  const list = handlers[event] || [];
  for (const handlerId of list) {
    const fn = (window as any)[`_${handlerId}`];
    if (typeof fn === "function") {
      fn({ event, id: 1, payload });
    }
  }
}

// ── Realistic Sample Payload: Next.js + Vercel + Tailwind + Stripe + GA4 ──
export const SAMPLE_PAYLOAD: StackPayload = {
  title: "Acme Cloud Platform",
  url: "https://acme.com",
  final_url: "https://acme.com/en",
  site_logo:
    "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect width='32' height='32' rx='8' fill='%230a84ff'/><text x='16' y='22' font-size='18' font-family='sans-serif' font-weight='bold' text-anchor='middle' fill='%23ffffff'>A</text></svg>",
  status_code: 200,
  lang: "en",
  description: "Next generation cloud development platform and distributed API infrastructure.",
  scanning: false,
  detections: [
    {
      id: "nextjs",
      name: "Next.js 15",
      category: "frontend",
      confidence: 0.99,
      status: "detected",
      evidence: ["window.__NEXT_DATA__ inline payload", "/_next/static/ JavaScript bundles in HTML"],
      website: "https://nextjs.org",
      logo: "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><circle cx='16' cy='16' r='16' fill='%23000000'/><path d='M22 24.5L13.5 12h-2v10.5h2v-8l7 10h1.5z' fill='%23ffffff'/></svg>",
    },
    {
      id: "react",
      name: "React 19",
      category: "frontend",
      confidence: 0.98,
      status: "detected",
      evidence: ["react-dom element containers", "React SSR hydration attributes in DOM tree"],
      website: "https://react.dev",
      logo: null, // Monogram fallback test
    },
    {
      id: "swr",
      name: "SWR",
      category: "frontend",
      confidence: 0.78,
      status: "likely",
      evidence: ["Client cache stale-while-revalidate polling pattern found in bundle"],
      website: "https://swr.vercel.app",
    },
    {
      id: "tailwind",
      name: "Tailwind CSS",
      category: "ui_library",
      confidence: 0.96,
      status: "detected",
      evidence: ["Utility class patterns in DOM classList", "Tailwind preflight CSS reset styles"],
      website: "https://tailwindcss.com",
      logo: null, // Monogram fallback test
    },
    {
      id: "radix_ui",
      name: "Radix UI Primitives",
      category: "ui_library",
      confidence: 0.89,
      status: "detected",
      evidence: ["data-radix-* attributes on modal dialogs and accessible popovers"],
      website: "https://www.radix-ui.com",
    },
    {
      id: "framer_motion",
      name: "Framer Motion",
      category: "ui_library",
      confidence: 0.72,
      status: "likely",
      evidence: ["Hardware-accelerated transform spring attributes in SVG hero elements"],
      website: "https://www.framer.com/motion",
    },
    {
      id: "vercel",
      name: "Vercel Edge Network",
      category: "hosting",
      confidence: 0.99,
      status: "detected",
      evidence: ["x-vercel-id HTTP header", "Vercel edge middleware response cookies"],
      website: "https://vercel.com",
      logo: "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><path d='M16 5l13 22H3z' fill='%23000000'/></svg>",
    },
    {
      id: "aws_s3",
      name: "Amazon S3",
      category: "hosting",
      confidence: 0.84,
      status: "detected",
      evidence: ["s3.amazonaws.com origin for static product media assets"],
      website: "https://aws.amazon.com/s3",
    },
    {
      id: "ga4",
      name: "Google Analytics 4",
      category: "analytics",
      confidence: 0.94,
      status: "detected",
      evidence: ["googletagmanager.com/gtag/js loaded", "Global gtag('config', 'G-XXXXXX') in DOM"],
      website: "https://analytics.google.com",
    },
    {
      id: "posthog",
      name: "PostHog Analytics",
      category: "analytics",
      confidence: 0.65,
      status: "likely",
      evidence: ["PostHog client ingestion ping to app.posthog.com/e/"],
      website: "https://posthog.com",
    },
    {
      id: "stripe",
      name: "Stripe Billing & Elements",
      category: "ecommerce",
      confidence: 0.96,
      status: "detected",
      evidence: ["js.stripe.com/v3 script loaded", "Stripe payment element iframe in DOM"],
      website: "https://stripe.com",
      logo: "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect width='32' height='32' rx='6' fill='%23635bff'/><path d='M15 11c-2 0-3 1-3 2.5 0 2.5 4 2 4 3.5 0 .6-.6 1-1.5 1-1.2 0-2.5-.5-3.5-1.2v3c1 .5 2.3.8 3.5.8 2.5 0 4-1.2 4-2.8 0-2.6-4-2.2-4-3.6 0-.5.5-.8 1.3-.8 1 0 2.2.4 3 .8v-2.8c-.8-.4-1.8-.6-2.8-.6z' fill='%23ffffff'/></svg>",
    },
    {
      id: "cloudflare",
      name: "Cloudflare CDN & WAF",
      category: "security",
      confidence: 0.82,
      status: "likely",
      evidence: ["cf-ray header on cached static media assets"],
      website: "https://www.cloudflare.com",
    },
    {
      id: "recaptcha",
      name: "Google reCAPTCHA Enterprise",
      category: "security",
      confidence: 0.45,
      status: "inferred",
      evidence: ["Grecaptcha enterprise token verification endpoint referenced in bundle"],
      website: "https://www.google.com/recaptcha",
    },
    {
      id: "hsts_header",
      name: "HTTP Strict Transport Security",
      category: "security",
      confidence: 1.0,
      status: "detected",
      evidence: ["Strict-Transport-Security: max-age=63072000; includeSubDomains; preload"],
    },
  ],
  groups: [
    {
      category: "frontend",
      label: "Frontend Frameworks",
      ids: ["nextjs", "react", "swr"],
    },
    {
      category: "ui_library",
      label: "UI Libraries & Styling",
      ids: ["tailwind", "radix_ui", "framer_motion"],
    },
    {
      category: "hosting",
      label: "Cloud & Hosting",
      ids: ["vercel", "aws_s3"],
    },
    {
      category: "ecommerce",
      label: "Payments & Commerce",
      ids: ["stripe"],
    },
    {
      category: "analytics",
      label: "Analytics & Telemetry",
      ids: ["ga4", "posthog"],
    },
    {
      category: "security",
      label: "Security & Edge Delivery",
      ids: ["cloudflare", "recaptcha", "hsts_header"],
    },
  ],
  security: {
    hsts: true,
    csp: false,
    x_frame_options: true,
    x_content_type_options: true,
    referrer_policy: true,
    permissions_policy: false,
  },
  stats: {
    html_bytes: 49152,
    script_count: 14,
    inline_script_count: 3,
    stylesheet_count: 2,
    image_count: 18,
    js_bytes_fetched: 348160,
    css_bytes_fetched: 63488,
    third_party_hosts: [
      "js.stripe.com",
      "googletagmanager.com",
      "app.posthog.com",
      "fonts.googleapis.com",
      "fonts.gstatic.com",
      "images.unsplash.com",
      "s3.amazonaws.com",
      "vitals.vercel-insights.com",
      "api.segment.io",
      "cdn.jsdelivr.net",
    ],
  },
  notes: [
    "Next.js App Router deployed on Vercel with edge runtime.",
    "Responsive design via Tailwind utility architecture.",
    "Integrated Stripe Checkout with global payment compliance.",
  ],
  sections: [
    {
      idx: 1,
      title: "Hero & Value Proposition",
      summary: "Dynamic SSR landing section showcasing platform metrics and customer quotes.",
      stack_refs: ["nextjs", "tailwind", "framer_motion"],
    },
    {
      idx: 2,
      title: "Feature Showcase",
      summary: "Interactive components using unstyled accessible Radix UI primitives.",
      stack_refs: ["react", "radix_ui"],
    },
    {
      idx: 3,
      title: "Tiered Pricing & Checkout",
      summary: "Embedded Stripe checkout interface routing to payment rails.",
      stack_refs: ["stripe"],
    },
    {
      idx: 4,
      title: "Telemetry & Performance Funnel",
      summary: "Client-side telemetry instrumentation tracking user engagement funnels.",
      stack_refs: ["ga4", "posthog"],
    },
  ],
  markdown: `# Reverse Engineering Report: Acme Cloud Platform
**URL:** https://acme.com/en (HTTP 200, English)

## Summary
Acme is a modern Next.js 15 web application deployed on Vercel with Tailwind CSS, Stripe billing, and Google Analytics 4.

## Tech Stack Detections
### Frontend Frameworks
- **Next.js 15** (Confirmed, 99%): window.__NEXT_DATA__ inline payload, /_next/static/ bundles
- **React 19** (Confirmed, 98%): react-dom containers, SSR hydration attributes
- **SWR** (Likely, 78%): Client cache stale-while-revalidate polling pattern

### UI Libraries & Styling
- **Tailwind CSS** (Confirmed, 96%): Utility class patterns in DOM classList
- **Radix UI** (Confirmed, 89%): data-radix-* attributes on modal dialogs
- **Framer Motion** (Likely, 72%): Hardware-accelerated transform spring attributes

### Cloud & Hosting
- **Vercel Edge Network** (Confirmed, 99%): x-vercel-id HTTP header
- **Amazon S3** (Confirmed, 84%): s3.amazonaws.com asset origin

### Payments & Commerce
- **Stripe** (Confirmed, 96%): js.stripe.com/v3 loaded, payment element iframe

### Analytics & Telemetry
- **Google Analytics 4** (Confirmed, 94%): googletagmanager.com/gtag/js loaded
- **PostHog** (Likely, 65%): PostHog client ingestion calls

### Security & Edge Delivery
- **Cloudflare** (Likely, 82%): cf-ray response header
- **Google reCAPTCHA** (Possible, 45%): grecaptcha token endpoint
- **HSTS** (Confirmed, 100%): max-age=63072000

## Security Headers (4/6)
- [x] HSTS
- [ ] CSP (Missing)
- [x] X-Frame-Options
- [x] X-Content-Type-Options
- [x] Referrer-Policy
- [ ] Permissions-Policy (Missing)

## Page Weight
- HTML: 48 KB
- JS: 340 KB (14 scripts, 3 inline)
- CSS: 62 KB (2 stylesheets)
- Images: 18 assets
- Third-party hosts: 10 hosts (js.stripe.com, googletagmanager.com, app.posthog.com, ...)
`,
  summary:
    "Acme is a modern Next.js 15 web application deployed on Vercel with Tailwind CSS, Stripe billing, and Google Analytics 4.",
};

function DevStackHarness() {
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [isScanning, setIsScanning] = useState(false);
  const [frameWidth, setFrameWidth] = useState<"380" | "520" | "full">("380");
  const [progressStep, setProgressStep] = useState(0);

  // Initialize store with sample payload
  useEffect(() => {
    useSidebar.getState().showStack(SAMPLE_PAYLOAD);
  }, []);

  const toggleTheme = () => {
    const next = theme === "dark" ? "light" : "dark";
    setTheme(next);
    document.documentElement.setAttribute("data-theme", next);
  };

  const fireProgress = () => {
    const nextStep = (progressStep % 5) + 1;
    setProgressStep(nextStep);
    const phases: StackProgress["phase"][] = ["starting", "scanning", "layout", "touring", "summary"];
    const textMap = [
      "Connecting to origin server...",
      "Analyzing script bundles and DOM structure...",
      "Extracting page layout and section coordinates...",
      "Touring key UI components...",
      "Synthesizing tech stack report...",
    ];

    emit("reverse:progress", {
      phase: phases[nextStep - 1],
      text: textMap[nextStep - 1],
      step: nextStep,
      total: 5,
    });
  };

  const focusCards = () => {
    emit("reverse:focus_card", { ids: ["nextjs", "stripe"] });
  };

  const clearFocus = () => {
    emit("reverse:focus_card", { ids: [] });
  };

  const toggleScanningState = () => {
    const next = !isScanning;
    setIsScanning(next);
    const updated: StackPayload = {
      ...SAMPLE_PAYLOAD,
      scanning: next,
      summary: next ? "" : SAMPLE_PAYLOAD.summary,
    };
    useSidebar.getState().showStack(updated);
    if (next) {
      emit("reverse:progress", {
        phase: "scanning",
        text: "Scanning website dependencies and security headers...",
        step: 2,
        total: 5,
      });
    } else {
      emit("reverse:progress", {
        phase: "done",
        text: "Analysis completed.",
        step: 5,
        total: 5,
      });
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", width: "100%" }}>
      {/* Dev control toolbar */}
      <div
        style={{
          background: "#181822",
          borderBottom: "1px solid rgba(255, 255, 255, 0.12)",
          padding: "8px 16px",
          display: "flex",
          gap: "10px",
          alignItems: "center",
          flexWrap: "wrap",
          zIndex: 100,
        }}
      >
        <span style={{ fontSize: "12px", fontWeight: 700, color: "#0a84ff", marginRight: "6px" }}>
          DEV HARNESS:
        </span>
        <button
          type="button"
          onClick={toggleTheme}
          style={{
            background: "#242432",
            color: "#fff",
            border: "1px solid rgba(255, 255, 255, 0.15)",
            padding: "5px 10px",
            borderRadius: "4px",
            cursor: "pointer",
            fontSize: "12px",
          }}
        >
          Toggle theme ({theme})
        </button>
        <button
          type="button"
          onClick={fireProgress}
          style={{
            background: "#242432",
            color: "#fff",
            border: "1px solid rgba(255, 255, 255, 0.15)",
            padding: "5px 10px",
            borderRadius: "4px",
            cursor: "pointer",
            fontSize: "12px",
          }}
        >
          Fire progress ({progressStep}/5)
        </button>
        <button
          type="button"
          onClick={focusCards}
          style={{
            background: "#242432",
            color: "#fff",
            border: "1px solid rgba(255, 255, 255, 0.15)",
            padding: "5px 10px",
            borderRadius: "4px",
            cursor: "pointer",
            fontSize: "12px",
          }}
        >
          Focus card (nextjs, stripe)
        </button>
        <button
          type="button"
          onClick={clearFocus}
          style={{
            background: "#242432",
            color: "#fff",
            border: "1px solid rgba(255, 255, 255, 0.15)",
            padding: "5px 10px",
            borderRadius: "4px",
            cursor: "pointer",
            fontSize: "12px",
          }}
        >
          Clear focus
        </button>
        <button
          type="button"
          onClick={toggleScanningState}
          style={{
            background: isScanning ? "#0a84ff" : "#242432",
            color: "#fff",
            border: "1px solid rgba(255, 255, 255, 0.15)",
            padding: "5px 10px",
            borderRadius: "4px",
            cursor: "pointer",
            fontSize: "12px",
          }}
        >
          Scanning state ({isScanning ? "ON" : "OFF"})
        </button>
        <div style={{ width: "1px", height: "20px", background: "rgba(255, 255, 255, 0.15)" }} />
        <button
          type="button"
          onClick={() => setFrameWidth("380")}
          style={{
            background: frameWidth === "380" ? "#0a84ff" : "#242432",
            color: "#fff",
            border: "1px solid rgba(255, 255, 255, 0.15)",
            padding: "5px 10px",
            borderRadius: "4px",
            cursor: "pointer",
            fontSize: "12px",
          }}
        >
          Phone (380px)
        </button>
        <button
          type="button"
          onClick={() => setFrameWidth("520")}
          style={{
            background: frameWidth === "520" ? "#0a84ff" : "#242432",
            color: "#fff",
            border: "1px solid rgba(255, 255, 255, 0.15)",
            padding: "5px 10px",
            borderRadius: "4px",
            cursor: "pointer",
            fontSize: "12px",
          }}
        >
          Dock (520px)
        </button>
        <button
          type="button"
          onClick={() => setFrameWidth("full")}
          style={{
            background: frameWidth === "full" ? "#0a84ff" : "#242432",
            color: "#fff",
            border: "1px solid rgba(255, 255, 255, 0.15)",
            padding: "5px 10px",
            borderRadius: "4px",
            cursor: "pointer",
            fontSize: "12px",
          }}
        >
          Full Width
        </button>
      </div>

      {/* Main preview body: render frame according to width setting */}
      <div
        style={{
          flex: 1,
          display: "flex",
          justifyContent: frameWidth === "full" ? "stretch" : "flex-start",
          background: theme === "dark" ? "#07070a" : "#e5e5ea",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: frameWidth === "380" ? "380px" : frameWidth === "520" ? "520px" : "100%",
            height: "100%",
            boxShadow: frameWidth === "full" ? "none" : "0 0 30px rgba(0,0,0,0.4)",
            display: "flex",
            flexDirection: "column",
            transition: "width 0.2s ease",
          }}
        >
          <StackView onDock={(d) => console.log("[DevStack] onDock called with:", d)} />
        </div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined") {
  const container = document.getElementById("root");
  if (container) {
    const root = createRoot(container);
    root.render(
      <StrictMode>
        <DevStackHarness />
      </StrictMode>,
    );
  }
}
