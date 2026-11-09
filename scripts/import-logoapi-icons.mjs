// Fetch product logos through the logoapi.sh MCP and bundle them into the app.
// Dev-time only: the released app reads local files and never calls logoapi.
//   node scripts/import-logoapi-icons.mjs            dry run into a scratch folder, prints a verdict per icon
//   node scripts/import-logoapi-icons.mjs --apply    replace frontend/public/activity-logos/<id>.svg + SOURCES.json
//   --only a,b    just these ids      --force   also apply REVIEW verdicts
// About 60 requests, one at a time with a pause (the service rate-limits bursts); never the whole catalog.
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = process.cwd();
const args = process.argv.slice(2);
const apply = args.includes("--apply");
const force = args.includes("--force");
const only = args.includes("--only") ? new Set(args[args.indexOf("--only") + 1].split(",")) : null;
const manifest = JSON.parse(await readFile(join(root, "scripts", "logoapi-icon-sources.json"), "utf8"));
const logos = join(root, "frontend", "public", "activity-logos");
const scratch = join(tmpdir(), "nexus-logoapi-dryrun");
const outDir = apply ? logos : scratch;
await mkdir(outDir, { recursive: true });

const MAX_BYTES = 40 * 1024;
const PAUSE_MS = 1100;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// Domains that stand for a whole company, never for one product.
const PARENT_DOMAINS = new Set(["microsoft.com", "google.com/chrome"]);
const SAME_MARK_OK = new Set(["openai"]);

async function rpc(method, params, session) {
  for (let attempt = 0; attempt < 6; attempt++) {
    const res = await fetch(manifest.mcp, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json, text/event-stream",
        ...(session ? { "Mcp-Session-Id": session } : {}),
      },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
      signal: AbortSignal.timeout(30000),
    });
    if (res.status === 429) { await sleep(3000 * (attempt + 1)); continue; }
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const text = await res.text();
    const data = text.trimStart().startsWith("{") ? text : (text.match(/^data:\s*(\{.*\})\s*$/m)?.[1] ?? "");
    const reply = JSON.parse(data);
    if (reply.error) throw new Error(JSON.stringify(reply.error));
    return reply.result;
  }
  throw new Error("rate limited (429) after retries");
}

function checkSvg(svg, id) {
  const refs = [...svg.matchAll(/(?:xlink:)?href\s*=\s*["']([^"']+)["']/gi)].map((m) => m[1]);
  if (!svg.startsWith("<svg") || !svg.trimEnd().endsWith("</svg>")
      || /<!doctype|<script|<foreignObject|\son[a-z]+\s*=|javascript:|url\(\s*["']?https?:/i.test(svg)
      || refs.some((r) => !r.startsWith("#") && !/^data:image\/png;base64,[A-Za-z0-9+/=]+$/.test(r))) {
    throw new Error("unsafe or external references");
  }
  if (!/viewBox\s*=/.test(svg)) throw new Error("no viewBox");
  if (Buffer.byteLength(svg) > MAX_BYTES) throw new Error(`too large (${Buffer.byteLength(svg)} B)`);
}
const stripProlog = (s) => s.replace(/<\?xml[^>]*\?>/g, "").replace(/<!--[\s\S]*?-->/g, "").replace(/<title>[\s\S]*?<\/title>/g, "");
const normalise = (svg) => `${stripProlog(svg).trim().replace(/<svg([^>]*?)\s(?:width|height)\s*=\s*"[^"]*"/g, "<svg$1").replace(/<svg([^>]*?)\s(?:width|height)\s*=\s*"[^"]*"/g, "<svg$1")}\n`;
const ratio = (svg) => {
  const m = svg.match(/viewBox\s*=\s*"\s*[-\d.]+[\s,]+[-\d.]+[\s,]+([\d.]+)[\s,]+([\d.]+)/);
  return m ? Number(m[1]) / Number(m[2]) : null;
};
const nameOk = (name, expect) => expect.some((e) => (e.startsWith("=") ? name === e.slice(1) : name.includes(e)));

const rows = [];
const hashes = new Map();
const session = null;
await rpc("initialize", { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "nexus-logo-import", version: "1.0" } }, session);

for (const [id, info] of Object.entries(manifest.icons)) {
  if (only && !only.has(id)) continue;
  const row = { id, domain: info.domain, verdict: "FAIL", note: "" };
  rows.push(row);
  try {
    if (!/^[a-z0-9]+$/.test(id) || PARENT_DOMAINS.has(info.domain)) throw new Error("parent-company or invalid id");
    const result = await rpc("tools/call", { name: "get_logo", arguments: { domain: info.domain, variant: "base", format: "svg" } }, session);
    const s = result?.structuredContent;
    if (!s?.found) throw new Error("not in logoapi");
    if (s.domain !== info.domain) throw new Error(`domain mismatch (${s.domain})`);
    if (!/^https:\/\/logoapi\.sh\//.test(s.url ?? "")) throw new Error("unexpected url host");
    row.name = s.name;
    if (!nameOk(String(s.name).toLowerCase(), info.expect)) throw new Error(`brand name "${s.name}" is not ${info.expect.join("/")}`);
    await sleep(PAUSE_MS);
    const file = await fetch(s.url, { signal: AbortSignal.timeout(30000) });
    if (!file.ok) throw new Error(`download HTTP ${file.status}`);
    const svg = normalise(await file.text());
    checkSvg(svg, id);
    const sha = createHash("sha256").update(svg).digest("hex");
    row.sha = sha; row.bytes = Buffer.byteLength(svg); row.url = s.url; row.svg = svg;
    const twin = hashes.get(sha);
    if (twin && !(SAME_MARK_OK.has(id) || SAME_MARK_OK.has(twin))) throw new Error(`identical to ${twin}`);
    hashes.set(sha, id);
    row.verdict = "OK";
    const old = existsSync(join(logos, `${id}.svg`)) ? await readFile(join(logos, `${id}.svg`), "utf8") : null;
    const [a, b] = [ratio(svg), old && ratio(old)];
    // logoapi serves full logos; many are wordmark lockups (wide), which turn into a tiny mark on a square tile.
    if (a && (a > 1.43 || a < 0.7)) { row.verdict = "WORDMARK"; row.note = `aspect ${a.toFixed(2)}, not a square mark - existing file kept`; }
    else if (a && b && Math.abs(a / b - 1) > 0.35) { row.verdict = "REVIEW"; row.note = `aspect ${a.toFixed(2)} vs old ${b.toFixed(2)}`; }
  } catch (e) {
    row.note = e instanceof Error ? e.message : String(e);
  }
  console.log(`${row.verdict.padEnd(6)} ${id.padEnd(14)} ${info.domain.padEnd(28)} ${String(row.name ?? "").padEnd(22)} ${String(row.bytes ?? "").padStart(6)}  ${row.note}`);
  await sleep(PAUSE_MS);
}

const write = rows.filter((r) => r.verdict === "OK" || (force && r.verdict === "REVIEW"));
for (const r of write) await writeFile(join(outDir, `${r.id}.svg`), r.svg);
console.log(`\n${rows.filter((r) => r.verdict === "OK").length} OK, ${rows.filter((r) => r.verdict === "REVIEW").length} REVIEW, ${rows.filter((r) => r.verdict === "FAIL").length} FAIL of ${rows.length}.`);
console.log(apply ? `Wrote ${write.length} files to ${outDir}` : `Dry run: ${write.length} files written to ${outDir} (nothing in the app changed).`);

if (apply) {
  const sourcesPath = join(logos, "SOURCES.json");
  const sources = existsSync(sourcesPath) ? JSON.parse(await readFile(sourcesPath, "utf8")) : {};
  const day = new Date().toISOString().slice(0, 10);
  for (const r of write) sources[r.id] = { source: "logoapi.sh", domain: r.domain, url: r.url, sha256: r.sha, fetched: day };
  await writeFile(sourcesPath, JSON.stringify(Object.fromEntries(Object.entries(sources).sort()), null, 2) + "\n");
}
