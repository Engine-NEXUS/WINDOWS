import { describe, expect, it } from "vitest";
import { deriveConnectionStates, MCP_REGISTRY, type McpConnectCard } from "./McpServersPanel";
import { McpIcons } from "./McpIcons";

const card = (server: string, state: McpConnectCard["state"]): McpConnectCard => ({
  server, state, note: "", steps: [], qr_image_uri: null, qr_code_text: null, pair_url: null,
});

describe("Connections registry", () => {
  it("has 14 unique providers with logos and excludes Google account sign-in", () => {
    const ids = MCP_REGISTRY.map((provider) => provider.id);
    expect(ids).toHaveLength(14);
    expect(new Set(ids).size).toBe(14);
    expect(ids).not.toContain("google");
    expect(ids).toEqual(expect.arrayContaining(["coderabbit", "telegram", "spotify", "amazon", "swiggy", "whatsapp"]));
    for (const provider of MCP_REGISTRY) expect(typeof McpIcons[provider.icon]).toBe("function");
    expect(MCP_REGISTRY.find((provider) => provider.id === "coderabbit")?.kind).toBe("cli");
  });

  it("supports Render and Vercel with tokens and connects when stored", () => {
    const unconfigured = deriveConnectionStates({ figma: "live", spotify: "live", github: "live" }, [], false, null);
    expect(unconfigured.figma).toBe("stored");
    expect(unconfigured.spotify).toBe("local");
    expect(unconfigured.github).toBe("needs_setup");
    expect(unconfigured.amazon).toBe("needs_setup");
    expect(unconfigured.vercel).toBe("needs_setup");
    expect(unconfigured.render).toBe("needs_setup");

    const configured = deriveConnectionStates({ render: "live", vercel: "live" }, [], false, null);
    expect(configured.render).toBe("connected");
    expect(configured.vercel).toBe("connected");
  });

  it("assigns valid categories, login URLs, and token URLs for deployment providers", () => {
    const categories = new Set(MCP_REGISTRY.map((p) => p.category));
    expect(Array.from(categories).sort()).toEqual([
      "Cloud & Deployments",
      "Commerce & Everyday Apps",
      "Developer & Engineering",
      "Messaging & Notifications",
    ]);

    const render = MCP_REGISTRY.find((p) => p.id === "render")!;
    expect(render.loginUrl).toBe("https://dashboard.render.com/login");
    expect(render.tokenUrl).toBe("https://dashboard.render.com/u/settings#api-keys");

    const vercel = MCP_REGISTRY.find((p) => p.id === "vercel")!;
    expect(vercel.loginUrl).toBe("https://vercel.com/login");
    expect(vercel.tokenUrl).toBe("https://vercel.com/account/tokens");
  });

  it("requires both Telegram token and owner chat ID to be configured", () => {
    expect(deriveConnectionStates({ telegram: "live" }, [], false, null).telegram).toBe("needs_setup");
    expect(deriveConnectionStates({ telegram: "live" }, [], false, null, undefined, "12345").telegram).toBe("configured");
  });

  it("aggregates Swiggy's three MCP services and reads lowercase backend states", () => {
    const partial = deriveConnectionStates({ swiggy: "live" }, [
      card("swiggy-food", "ready"), card("swiggy-instamart", "auth_required"), card("swiggy-dineout", "down"),
    ], false, null);
    expect(partial.swiggy).toBe("partial");
    const ready = deriveConnectionStates({ swiggy: "live" }, [
      card("swiggy-food", "ready"), card("swiggy-instamart", "ready"), card("swiggy-dineout", "ready"),
    ], false, null);
    expect(ready.swiggy).toBe("connected");
  });

  it("requires pairing for WhatsApp and labels Amazon transport separately", () => {
    const states = deriveConnectionStates({}, [card("whatsapp", "auth_required"), card("amazon", "ready")], false, null);
    expect(states.whatsapp).toBe("needs_setup");
    expect(states.amazon).toBe("available");
    expect(deriveConnectionStates({}, [card("whatsapp", "ready")], false, null).whatsapp).toBe("connected");
  });

  it("uses real GitHub and CodeRabbit auth results", () => {
    const states = deriveConnectionStates({}, [], true, {
      state: "connected", message: "", region: "us", organization: null,
    });
    expect(states.github).toBe("connected");
    expect(states.coderabbit).toBe("connected");
    expect(deriveConnectionStates({}, [], null, { state: "error", message: "", region: null, organization: null }).coderabbit).toBe("error");
  });

  it("shows unavailable rather than disconnected when status probes fail", () => {
    const states = deriveConnectionStates(
      { figma: "live", swiggy: "live" }, [], null, null,
      { vault: false, mcp: false, coderabbit: false },
    );
    expect(states.figma).toBe("error");
    expect(states.swiggy).toBe("error");
    expect(states.whatsapp).toBe("error");
    expect(states.amazon).toBe("error");
    expect(states.coderabbit).toBe("error");
  });
});
