// A small highlighter for reading code: keywords, strings, comments,
// numbers and type-like names, line by line, by file extension. Tokens are
// rendered as text (never HTML), so whatever a file holds stays text.

export interface Token {
  t: string;
  c: "" | "k" | "s" | "c" | "n" | "y" | "f" | "p";
}

const kw = (s: string) => new Set(s.split(" "));
const LANGS: Record<string, { line: string[]; kw: Set<string> }> = {
  rust: { line: ["//"], kw: kw("as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while") },
  ts: { line: ["//"], kw: kw("as async await break case catch class const continue default delete do else enum export extends false finally for from function if import in instanceof interface let new null of return static super switch this throw true try type typeof undefined var void while yield") },
  py: { line: ["#"], kw: kw("and as assert async await break class continue def del elif else except False finally for from global if import in is lambda None nonlocal not or pass raise return self True try while with yield") },
  go: { line: ["//"], kw: kw("break case chan const continue default defer else fallthrough for func go goto if import interface map nil package range return select struct switch true false type var") },
  sh: { line: ["#"], kw: kw("case do done elif else esac export fi for function if in local return set then until while") },
  toml: { line: ["#"], kw: kw("true false") },
  yaml: { line: ["#"], kw: kw("true false null yes no") },
  plain: { line: [], kw: new Set() },
};
const EXT: Record<string, string> = {
  rs: "rust", ts: "ts", tsx: "ts", js: "ts", jsx: "ts", mjs: "ts", svelte: "ts", java: "ts", kt: "ts", swift: "ts", c: "ts", h: "ts", cpp: "ts", cs: "ts",
  py: "py", go: "go", sh: "sh", bash: "sh", zsh: "sh", toml: "toml", yml: "yaml", yaml: "yaml", json: "ts",
};

export const language = (path: string) => EXT[path.split(".").pop()?.toLowerCase() ?? ""] ?? "plain";

const WORD = /^[A-Za-z_][A-Za-z0-9_]*/;
const NUM = /^(0x[0-9a-fA-F_]+|\d[\d_]*(\.\d+)?([eE][+-]?\d+)?)/;

/** One line's tokens. Block comments are not followed across lines. */
export function tokens(line: string, lang: string): Token[] {
  const L = LANGS[lang] ?? LANGS.plain!;
  if (lang === "plain") return [{ t: line, c: "" }];
  const out: Token[] = [];
  let i = 0;
  const push = (t: string, c: Token["c"]) => {
    const last = out[out.length - 1];
    if (last && last.c === c) last.t += t;
    else out.push({ t, c });
  };
  while (i < line.length) {
    const rest = line.slice(i);
    const comment = L.line.find((p) => rest.startsWith(p)) ?? (rest.startsWith("/*") && lang !== "py" && lang !== "sh" ? "/*" : null);
    if (comment) {
      push(rest, "c");
      break;
    }
    const q = rest[0];
    if (q === '"' || q === "'" || q === "`") {
      // A Rust lifetime ('a) is not a string.
      if (q === "'" && lang === "rust" && /^'[a-z_]+\b(?!')/.test(rest)) {
        const m = rest.match(/^'[a-z_]+/)!;
        push(m[0], "y");
        i += m[0].length;
        continue;
      }
      let j = 1;
      while (j < rest.length && rest[j] !== q) j += rest[j] === "\\" ? 2 : 1;
      push(rest.slice(0, j + 1), "s");
      i += j + 1;
      continue;
    }
    const n = rest.match(NUM);
    if (n && !/[A-Za-z_]/.test(line[i - 1] ?? "")) {
      push(n[0], "n");
      i += n[0].length;
      continue;
    }
    const w = rest.match(WORD);
    if (w) {
      const word = w[0];
      const next = rest.slice(word.length).trimStart()[0];
      if (lang === "toml" && /^\s*$/.test(line.slice(0, i)) && next === "=") push(word, "f");
      else if (L.kw.has(word)) push(word, "k");
      else if (/^[A-Z][a-z0-9]+[A-Za-z0-9]*$/.test(word)) push(word, "y");
      else if (next === "(" || (lang === "rust" && next === "!")) push(word, "f");
      else push(word, "");
      i += word.length;
      continue;
    }
    push(line[i]!, /[{}()[\];,.:<>=+\-*/&|!?@#%^~]/.test(line[i]!) ? "p" : "");
    i += 1;
  }
  return out;
}
