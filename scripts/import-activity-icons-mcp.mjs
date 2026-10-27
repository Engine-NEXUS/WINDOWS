// Import the reviewed Activity Manager catalog through the pinned theSVG MCP.
// Existing SVGs are never overwritten. This runs only during development;
// the released app reads local assets and makes no icon-service requests.
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const root = process.cwd();
const manifest = JSON.parse(await readFile(
  join(root, "scripts", "activity-icon-sources.json"), "utf8"));
if (manifest.package !== "@thesvg/mcp-server@0.8.4") {
  throw new Error("Unreviewed theSVG MCP package version");
}
const allowedLicenses = new Set(["CC0-1.0", "MIT", "Apache-2.0"]);
const output = join(root, "frontend", "public", "activity-logos");
await mkdir(output, { recursive: true });
const entries = Object.entries(manifest.icons).filter(([asset, info]) => {
  if (!/^[a-z0-9]+$/.test(asset) || !/^[a-z0-9-]+$/.test(info.slug)
      || !/^[a-zA-Z0-9]+$/.test(info.variant)
      || !allowedLicenses.has(info.license)) {
    throw new Error(`Unreviewed icon source: ${asset}`);
  }
  return !existsSync(join(output, `${asset}.svg`));
});
if (!entries.length) {
  console.log("All reviewed Activity Manager icons are already present.");
  process.exit(0);
}

const command = process.platform === "win32" ? "cmd.exe" : "npx";
const args = process.platform === "win32"
  ? ["/d", "/c", "npx", "-y", manifest.package]
  : ["-y", manifest.package];
const child = spawn(command, args, { stdio: ["pipe", "pipe", "pipe"] });
const pending = new Map();
let nextId = 1;
let buffer = "";
let stderr = "";
child.stderr.on("data", (data) => { stderr = (stderr + data.toString()).slice(-2000); });
child.on("error", (error) => {
  for (const { reject } of pending.values()) reject(error);
  pending.clear();
});
child.on("exit", (code) => {
  for (const { reject } of pending.values()) reject(new Error(`MCP exited (${code}): ${stderr}`));
  pending.clear();
});
child.stdout.on("data", (data) => {
  buffer += data.toString();
  let end;
  while ((end = buffer.indexOf("\n")) !== -1) {
    const line = buffer.slice(0, end).trim();
    buffer = buffer.slice(end + 1);
    if (!line) continue;
    let reply;
    try { reply = JSON.parse(line); } catch { continue; }
    const waiter = pending.get(reply.id);
    if (!waiter) continue;
    pending.delete(reply.id);
    clearTimeout(waiter.timer);
    if (reply.error) waiter.reject(new Error(JSON.stringify(reply.error)));
    else waiter.resolve(reply.result);
  }
});
function call(method, params, timeoutMs = 30000) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`${method} timed out`));
    }, timeoutMs);
    pending.set(id, { resolve, reject, timer });
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });
}
function cleanSvg(response, asset) {
  if (response?.isError) throw new Error(response.content?.[0]?.text ?? `Failed to get ${asset}`);
  const body = response?.content?.[0]?.text ?? "";
  const svg = body.match(/```svg\s*([\s\S]*?)\s*```/)?.[1];
  const references = [...(svg?.matchAll(/(?:xlink:)?href\s*=\s*["']([^"']+)["']/gi) ?? [])]
    .map((match) => match[1]);
  if (!svg?.startsWith("<svg") || !svg.trimEnd().endsWith("</svg>")
      || /<!doctype|<script|<foreignObject|\son[a-z]+\s*=|javascript:|url\(\s*["']?https?:/i.test(svg)
      || references.some((ref) => !ref.startsWith("#") && !/^data:image\/png;base64,[A-Za-z0-9+/=]+$/.test(ref))) {
    throw new Error(`Rejected invalid or externally linked SVG for ${asset}`);
  }
  return `${svg}\n`;
}

let imported = 0;
const failures = [];
try {
  await call("initialize", {
    protocolVersion: "2025-03-26", capabilities: {},
    clientInfo: { name: "nexus-activity-icon-import", version: "1.0" },
  });
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
  let cursor = 0;
  async function worker() {
    while (cursor < entries.length) {
      const [asset, info] = entries[cursor++];
      try {
        const result = await call("tools/call", {
          name: "get_icon", arguments: { slug: info.slug, variant: info.variant },
        });
        const svg = cleanSvg(result, asset);
        await writeFile(join(output, `${asset}.svg`), svg, { flag: "wx" });
        imported++;
        console.log(`${imported}/${entries.length} ${asset} (${info.slug})`);
      } catch (error) {
        failures.push(`${asset}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  }
  await Promise.all(Array.from({ length: Math.min(4, entries.length) }, () => worker()));
} finally {
  child.kill();
}
console.log(`Imported ${imported} local icons; ${failures.length} failed.`);
if (failures.length) {
  for (const failure of failures) console.error(failure);
  process.exitCode = 1;
}
