// What a file is, at a glance: a short label and its language's colour,
// as editors mark them. By whole name first (Cargo.lock, Dockerfile), then
// by extension; anything else is a plain page.

export interface FileType {
  label: string;
  color: string;
  name: string;
}

const BY_NAME: Record<string, FileType> = {
  "cargo.toml": { label: "RS", color: "#dea584", name: "Cargo manifest" },
  "cargo.lock": { label: "LK", color: "#8e8e93", name: "Lockfile" },
  "package.json": { label: "{}", color: "#cb3837", name: "npm package" },
  "package-lock.json": { label: "LK", color: "#8e8e93", name: "Lockfile" },
  "pnpm-lock.yaml": { label: "LK", color: "#8e8e93", name: "Lockfile" },
  "yarn.lock": { label: "LK", color: "#8e8e93", name: "Lockfile" },
  dockerfile: { label: "DK", color: "#2496ed", name: "Dockerfile" },
  makefile: { label: "MK", color: "#6d8086", name: "Makefile" },
  justfile: { label: "JF", color: "#6d8086", name: "justfile" },
  license: { label: "§", color: "#8e8e93", name: "Licence" },
  ".gitignore": { label: "GI", color: "#f05032", name: "Git ignore" },
  "speccoder.toml": { label: "✓", color: "#4f46e5", name: "speccoder configuration" },
};

const BY_EXT: Record<string, FileType> = {
  rs: { label: "RS", color: "#e57a44", name: "Rust" },
  ts: { label: "TS", color: "#3178c6", name: "TypeScript" },
  tsx: { label: "TX", color: "#3178c6", name: "TypeScript JSX" },
  js: { label: "JS", color: "#e5b800", name: "JavaScript" },
  mjs: { label: "JS", color: "#e5b800", name: "JavaScript" },
  jsx: { label: "JX", color: "#e5b800", name: "JavaScript JSX" },
  svelte: { label: "SV", color: "#ff3e00", name: "Svelte" },
  vue: { label: "VU", color: "#41b883", name: "Vue" },
  py: { label: "PY", color: "#3572a5", name: "Python" },
  go: { label: "GO", color: "#00add8", name: "Go" },
  java: { label: "JV", color: "#b07219", name: "Java" },
  kt: { label: "KT", color: "#a97bff", name: "Kotlin" },
  swift: { label: "SW", color: "#f05138", name: "Swift" },
  rb: { label: "RB", color: "#cc342d", name: "Ruby" },
  c: { label: "C", color: "#555fbf", name: "C" },
  h: { label: "H", color: "#555fbf", name: "C header" },
  cpp: { label: "C+", color: "#f34b7d", name: "C++" },
  cs: { label: "C#", color: "#178600", name: "C#" },
  php: { label: "PH", color: "#777bb4", name: "PHP" },
  sh: { label: "$", color: "#4eaa25", name: "Shell" },
  bash: { label: "$", color: "#4eaa25", name: "Shell" },
  zsh: { label: "$", color: "#4eaa25", name: "Shell" },
  sql: { label: "SQ", color: "#e38c00", name: "SQL" },
  md: { label: "M↓", color: "#6e7681", name: "Markdown" },
  mdx: { label: "M↓", color: "#6e7681", name: "MDX" },
  txt: { label: "TX", color: "#8e8e93", name: "Text" },
  json: { label: "{}", color: "#cbb33b", name: "JSON" },
  toml: { label: "TM", color: "#9c6b4e", name: "TOML" },
  yml: { label: "YM", color: "#cb171e", name: "YAML" },
  yaml: { label: "YM", color: "#cb171e", name: "YAML" },
  xml: { label: "<>", color: "#e37933", name: "XML" },
  html: { label: "<>", color: "#e34c26", name: "HTML" },
  css: { label: "#", color: "#663399", name: "CSS" },
  scss: { label: "#", color: "#c6538c", name: "SCSS" },
  svg: { label: "SV", color: "#ffb13b", name: "SVG" },
  png: { label: "IMG", color: "#a259ff", name: "Image" },
  jpg: { label: "IMG", color: "#a259ff", name: "Image" },
  jpeg: { label: "IMG", color: "#a259ff", name: "Image" },
  gif: { label: "IMG", color: "#a259ff", name: "Image" },
  webp: { label: "IMG", color: "#a259ff", name: "Image" },
  pdf: { label: "PDF", color: "#e5252a", name: "PDF" },
  lock: { label: "LK", color: "#8e8e93", name: "Lockfile" },
  env: { label: "ENV", color: "#ecd53f", name: "Environment" },
  proto: { label: "PB", color: "#4285f4", name: "Protocol Buffers" },
  graphql: { label: "GQ", color: "#e10098", name: "GraphQL" },
  tf: { label: "TF", color: "#7b42bc", name: "Terraform" },
  dart: { label: "DT", color: "#00b4ab", name: "Dart" },
  zig: { label: "ZG", color: "#ec915c", name: "Zig" },
  ex: { label: "EX", color: "#6e4a7e", name: "Elixir" },
  hs: { label: "HS", color: "#5e5086", name: "Haskell" },
  lua: { label: "LU", color: "#000080", name: "Lua" },
};

const PLAIN: FileType = { label: "", color: "#8e8e93", name: "File" };

/** The type of the file at `path` (a path or a bare name). */
export function fileType(path: string): FileType {
  const name = path.split("/").pop()?.toLowerCase() ?? "";
  if (BY_NAME[name]) return BY_NAME[name];
  if (name.startsWith("readme")) return BY_EXT.md!;
  if (name.startsWith(".env")) return BY_EXT.env!;
  const dot = name.lastIndexOf(".");
  return (dot > 0 && BY_EXT[name.slice(dot + 1)]) || PLAIN;
}
