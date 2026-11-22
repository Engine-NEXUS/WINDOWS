import { lexer, type Token, type Tokens } from "marked";

/**
 * The plan an agent wants approved is markdown (what Claude shows for /plan). The island draws it as real text:
 * headings, lists, bold, code, tables. It is turned into plain data here and the page builds elements from that data,
 * so no HTML string is ever made or inserted: raw HTML in a plan, links and images are shown as their text.
 */

export type Inline =
  | { t: "text"; text: string }
  | { t: "strong" | "em" | "del"; children: Inline[] }
  | { t: "code"; text: string }
  | { t: "br" };

export type Block =
  | { t: "heading"; level: 1 | 2 | 3 | 4; inlines: Inline[] }
  | { t: "paragraph"; inlines: Inline[] }
  | { t: "list"; ordered: boolean; start: number; items: { blocks: Block[]; checked: boolean | null }[] }
  | { t: "code"; text: string; lang: string }
  | { t: "quote"; blocks: Block[] }
  | { t: "table"; head: Inline[][]; rows: Inline[][][] }
  | { t: "rule" };

const MAX_DEPTH = 6;
const MAX_BLOCKS = 400;
const MAX_INPUT = 20_000;

function inlines(tokens: Token[] | undefined, depth = 0): Inline[] {
  const out: Inline[] = [];
  if (!tokens || depth > MAX_DEPTH) return out;
  for (const tk of tokens) {
    switch (tk.type) {
      case "strong":
      case "em":
      case "del":
        out.push({ t: tk.type, children: inlines((tk as Tokens.Strong).tokens, depth + 1) });
        break;
      case "codespan":
        out.push({ t: "code", text: (tk as Tokens.Codespan).text });
        break;
      case "br":
        out.push({ t: "br" });
        break;
      case "link":
        out.push(...inlines((tk as Tokens.Link).tokens, depth + 1));
        break;
      case "image":
        out.push({ t: "text", text: (tk as Tokens.Image).text });
        break;
      case "text":
      case "escape": {
        const x = tk as Tokens.Text;
        if (x.tokens && x.tokens.length > 0) out.push(...inlines(x.tokens, depth + 1));
        else out.push({ t: "text", text: x.text });
        break;
      }
      default: {
        // raw html and anything unknown: its visible text only, never markup
        const raw = (tk as { text?: string; raw?: string }).text ?? "";
        if (raw) out.push({ t: "text", text: raw.replace(/<[^>]*>/g, "") });
      }
    }
  }
  return out;
}

function blocks(tokens: Token[] | undefined, depth = 0): Block[] {
  const out: Block[] = [];
  if (!tokens || depth > MAX_DEPTH) return out;
  for (const tk of tokens) {
    if (out.length >= MAX_BLOCKS) break;
    switch (tk.type) {
      case "heading": {
        const h = tk as Tokens.Heading;
        out.push({ t: "heading", level: Math.min(4, Math.max(1, h.depth)) as 1 | 2 | 3 | 4, inlines: inlines(h.tokens) });
        break;
      }
      case "paragraph":
        out.push({ t: "paragraph", inlines: inlines((tk as Tokens.Paragraph).tokens) });
        break;
      case "text": {
        // a loose list item's text
        const x = tk as Tokens.Text;
        out.push({ t: "paragraph", inlines: x.tokens && x.tokens.length > 0 ? inlines(x.tokens) : [{ t: "text", text: x.text }] });
        break;
      }
      case "list": {
        const l = tk as Tokens.List;
        out.push({
          t: "list",
          ordered: l.ordered,
          start: typeof l.start === "number" ? l.start : 1,
          items: l.items.map((it) => ({ blocks: blocks(it.tokens, depth + 1), checked: it.task ? it.checked === true : null })),
        });
        break;
      }
      case "code": {
        const c = tk as Tokens.Code;
        out.push({ t: "code", text: c.text, lang: (c.lang ?? "").split(/\s+/)[0].slice(0, 20) });
        break;
      }
      case "blockquote":
        out.push({ t: "quote", blocks: blocks((tk as Tokens.Blockquote).tokens, depth + 1) });
        break;
      case "table": {
        const tb = tk as Tokens.Table;
        out.push({ t: "table", head: tb.header.map((c) => inlines(c.tokens)), rows: tb.rows.map((r) => r.map((c) => inlines(c.tokens))) });
        break;
      }
      case "hr":
        out.push({ t: "rule" });
        break;
      case "html": {
        const text = ((tk as Tokens.HTML).text ?? "").replace(/<[^>]*>/g, "").trim();
        if (text) out.push({ t: "paragraph", inlines: [{ t: "text", text }] });
        break;
      }
      default:
        break; // space and anything unknown
    }
  }
  return out;
}

/** The plan as blocks. Never throws: text that cannot be read as markdown comes back as one paragraph. */
export function planBlocks(markdown: string): Block[] {
  const src = (markdown ?? "").slice(0, MAX_INPUT);
  try {
    return blocks(lexer(src, { gfm: true, breaks: true }));
  } catch {
    return src.trim() ? [{ t: "paragraph", inlines: [{ t: "text", text: src }] }] : [];
  }
}

/** The visible words of a block list, for tests and for a one-line fallback. */
export function plainText(bs: Block[]): string {
  const il = (xs: Inline[]): string => xs.map((x) => (x.t === "text" || x.t === "code" ? x.text : x.t === "br" ? " " : il(x.children))).join("");
  return bs
    .map((b) => {
      switch (b.t) {
        case "heading":
        case "paragraph":
          return il(b.inlines);
        case "list":
          return b.items.map((i) => plainText(i.blocks)).join(" ");
        case "code":
          return b.text;
        case "quote":
          return plainText(b.blocks);
        case "table":
          return [...b.head, ...b.rows.flat()].map(il).join(" ");
        default:
          return "";
      }
    })
    .join(" ");
}
