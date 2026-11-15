/** A deliberately small syntax highlighter for the island's code pane: strings, comments, numbers, keywords, types
 *  and calls, for the languages coding agents mostly touch. Pure; no DOM. */

export type TokenClass = "kw" | "str" | "num" | "fn" | "type" | "com" | "plain";

export interface Token {
  text: string;
  cls: TokenClass;
}

export interface Language {
  /** The two to four letters on the file tab ("TS", "RS", "PY"). */
  label: string;
  /** Line-comment marker, if the language has one. */
  comment: string | null;
}

const BY_EXT: Record<string, Language> = {
  ts: { label: "TS", comment: "//" },
  tsx: { label: "TSX", comment: "//" },
  js: { label: "JS", comment: "//" },
  jsx: { label: "JSX", comment: "//" },
  mjs: { label: "JS", comment: "//" },
  cjs: { label: "JS", comment: "//" },
  rs: { label: "RS", comment: "//" },
  go: { label: "GO", comment: "//" },
  java: { label: "JV", comment: "//" },
  kt: { label: "KT", comment: "//" },
  swift: { label: "SW", comment: "//" },
  c: { label: "C", comment: "//" },
  h: { label: "H", comment: "//" },
  cpp: { label: "C++", comment: "//" },
  cs: { label: "C#", comment: "//" },
  py: { label: "PY", comment: "#" },
  rb: { label: "RB", comment: "#" },
  sh: { label: "SH", comment: "#" },
  ps1: { label: "PS", comment: "#" },
  toml: { label: "TOML", comment: "#" },
  yml: { label: "YML", comment: "#" },
  yaml: { label: "YML", comment: "#" },
  json: { label: "JSON", comment: null },
  md: { label: "MD", comment: null },
  css: { label: "CSS", comment: null },
  html: { label: "HTML", comment: null },
  sql: { label: "SQL", comment: "--" },
};

export function languageOf(path: string): Language {
  const m = /\.([A-Za-z0-9+#]+)$/.exec(path);
  const ext = m ? m[1].toLowerCase() : "";
  return BY_EXT[ext] ?? { label: ext ? ext.slice(0, 4).toUpperCase() : "TXT", comment: null };
}

const KEYWORDS = new Set(
  (
    "import export from as default const let var function return if else for while do switch case break continue " +
    "class extends implements interface type enum new this super try catch finally throw async await yield typeof " +
    "instanceof in of void null undefined true false static public private protected readonly abstract " +
    "fn pub mut use mod impl trait struct match loop where ref move crate self Self unsafe dyn " +
    "def lambda pass with is not and or None True False elif raise global " +
    "func package go defer chan select range"
  ).split(" "),
);

const TYPE_HINT = /^[A-Z][A-Za-z0-9_]*$/;

/** Split one line into tokens. */
export function tokenize(line: string, lang: Language): Token[] {
  const out: Token[] = [];
  let plain = "";
  const flush = () => {
    if (plain) out.push({ text: plain, cls: "plain" });
    plain = "";
  };
  let i = 0;
  while (i < line.length) {
    const rest = line.slice(i);
    if (lang.comment && rest.startsWith(lang.comment)) {
      flush();
      out.push({ text: rest, cls: "com" });
      break;
    }
    const c = line[i];
    if (c === '"' || c === "'" || c === "`") {
      flush();
      let j = i + 1;
      while (j < line.length && line[j] !== c) j += line[j] === "\\" ? 2 : 1;
      out.push({ text: line.slice(i, Math.min(j + 1, line.length)), cls: "str" });
      i = j + 1;
      continue;
    }
    const num = /^\d[\d_]*(\.\d+)?([eE][+-]?\d+)?/.exec(rest);
    if (num && !/[A-Za-z0-9_]/.test(line[i - 1] ?? " ")) {
      flush();
      out.push({ text: num[0], cls: "num" });
      i += num[0].length;
      continue;
    }
    const word = /^[A-Za-z_$][A-Za-z0-9_$]*/.exec(rest);
    if (word) {
      const w = word[0];
      const after = line[i + w.length];
      flush();
      if (KEYWORDS.has(w)) out.push({ text: w, cls: "kw" });
      else if (after === "(") out.push({ text: w, cls: "fn" });
      else if (TYPE_HINT.test(w)) out.push({ text: w, cls: "type" });
      else out.push({ text: w, cls: "plain" });
      i += w.length;
      continue;
    }
    plain += c;
    i++;
  }
  flush();
  return out;
}
