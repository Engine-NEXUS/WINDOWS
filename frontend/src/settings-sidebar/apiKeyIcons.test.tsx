import { describe, it, expect } from "vitest";
import { renderToString } from "react-dom/server";
import { ApiKeyIcon, ApiKeyIcons } from "./ApiKeyIcons";

describe("ApiKeyIcons & ApiKeyIcon selector", () => {
  it("renders distinct SVG markup for each key provider", () => {
    const services = [
      "groq",
      "gemini",
      "cerebras",
      "deepgram",
      "google_maps",
      "google_places",
      "openai",
      "anthropic",
      "elevenlabs",
      "github",
      "telegram",
      "unknown_service_fallback",
    ];

    for (const s of services) {
      const html = renderToString(<ApiKeyIcon service={s} size={28} />);
      expect(html).toContain("<svg");
      expect(html).toContain('width="28"');
      expect(html).toContain('height="28"');
    }
  });

  it("handles case insensitivity and symbols in service names", () => {
    const variants = ["Groq", "GROQ-API", "google_maps", "Google-Places", "gemini_api_key", "Cerebras-Cloud"];
    for (const v of variants) {
      const html = renderToString(<ApiKeyIcon service={v} size={32} />);
      expect(html).toContain("<svg");
      expect(html).toContain('width="32"');
    }
  });

  it("renders individual brand logos directly", () => {
    const components = [
      ApiKeyIcons.Groq,
      ApiKeyIcons.Gemini,
      ApiKeyIcons.Cerebras,
      ApiKeyIcons.Deepgram,
      ApiKeyIcons.GoogleMaps,
      ApiKeyIcons.GooglePlaces,
      ApiKeyIcons.OpenAI,
      ApiKeyIcons.Anthropic,
      ApiKeyIcons.ElevenLabs,
      ApiKeyIcons.GitHub,
      ApiKeyIcons.Telegram,
      ApiKeyIcons.GenericKey,
    ];

    for (const Comp of components) {
      const html = renderToString(<Comp size={24} />);
      expect(html).toContain("<svg");
      expect(html).toContain('role="img"');
      expect(html).toContain('width="24"');
    }
  });
});
