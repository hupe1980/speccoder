// A word-level diff of two lines: what a requirement said when its task was
// ticked, and what it says now. Longest common subsequence over words and
// the spaces between them; requirement lines are short.

export interface Segment {
  text: string;
  kind: "same" | "gone" | "new";
}

/** `- **FR-001**: Users MUST …` → `Users MUST …`: the wording, not the markup. */
export const wording = (line: string | null) => (line ?? "").replace(/^\s*-\s*\*\*[A-Z]+-\d+\*\*:?\s*/, "").trim();

export function diffWords(before: string, after: string): Segment[] {
  const a = before.split(/(\s+)/).filter(Boolean);
  const b = after.split(/(\s+)/).filter(Boolean);
  const w = b.length + 1;
  // lcs[i * w + j]: the common length of a[i..] and b[j..].
  const lcs = new Uint16Array((a.length + 1) * w);
  const at = (i: number, j: number) => lcs[i * w + j] ?? 0;
  for (let i = a.length - 1; i >= 0; i--)
    for (let j = b.length - 1; j >= 0; j--) lcs[i * w + j] = a[i] === b[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
  const out: Segment[] = [];
  const push = (text: string | undefined, kind: Segment["kind"]) => {
    if (text === undefined) return;
    const last = out[out.length - 1];
    if (last?.kind === kind) last.text += text;
    else out.push({ text, kind });
  };
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      push(a[i], "same");
      i++;
      j++;
    } else if (at(i + 1, j) >= at(i, j + 1)) push(a[i++], "gone");
    else push(b[j++], "new");
  }
  while (i < a.length) push(a[i++], "gone");
  while (j < b.length) push(b[j++], "new");
  return out;
}
