// One-time asset import. Supply the extracted icons.json files from the pinned
// @iconify-json/logos and @iconify-json/vscode-icons packages (see asset README).
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

const [logosPath, officePath] = process.argv.slice(2);
if (!logosPath || !officePath) {
  throw new Error("Usage: node scripts/vendor-activity-icons.mjs <logos/icons.json> <vscode-icons/icons.json>");
}
const [logos, office] = await Promise.all([logosPath, officePath].map(async (path) =>
  JSON.parse(await readFile(path, "utf8"))));
const chosen = {
  balena: [logos, "balena"],
  brave: [logos, "brave"],
  chrome: [logos, "chrome"],
  claude: [logos, "claude-icon"],
  cursor: [logos, "cursor-icon"],
  discord: [logos, "discord-icon"],
  edge: [logos, "microsoft-edge"],
  excel: [office, "file-type-excel"],
  fileexplorer: [office, "default-folder"],
  firefox: [logos, "firefox"],
  instagram: [logos, "instagram-icon"],
  notepad: [office, "file-type-text"],
  opencode: [logos, "opencode-icon"],
  postman: [logos, "postman-icon"],
  powerpoint: [office, "file-type-powerpoint"],
  slack: [logos, "slack-icon"],
  spotify: [logos, "spotify-icon"],
  teams: [logos, "microsoft-teams"],
  terminal: [logos, "terminal"],
  tiktok: [logos, "tiktok-icon"],
  twitter: [logos, "twitter"],
  vscode: [logos, "visual-studio-code"],
  visualstudio: [logos, "visual-studio"],
  whatsapp: [logos, "whatsapp-icon"],
  word: [office, "file-type-word"],
  youtube: [logos, "youtube-icon"],
  zoom: [logos, "zoom-icon"],
};
const output = join(process.cwd(), "frontend", "public", "activity-logos");
await mkdir(output, { recursive: true });
for (const [name, [set, slug]] of Object.entries(chosen)) {
  const icon = set.icons[slug];
  if (!icon?.body) throw new Error(`Missing icon: ${slug}`);
  const width = icon.width ?? set.width ?? 24;
  const height = icon.height ?? set.height ?? 24;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}">${icon.body}</svg>\n`;
  await writeFile(join(output, `${name}.svg`), svg);
}
console.log(`Wrote ${Object.keys(chosen).length} local Activity Manager SVG icons.`);
