// A document's outline: its H2 and H3 headings with stable ids, and
// the requirement IDs each section names — read from the host's rendered
// HTML (which carries no ids of its own).

export interface Section {
  id: string;
  level: 2 | 3;
  text: string;
  /** Requirement IDs (FR-001, SC-002) named in the section, once each. */
  reqs: string[];
}

const slug = (t: string) =>
  t
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60) || "section";

/** The HTML with an id on every H2/H3, and the outline. */
export function outline(html: string): { html: string; sections: Section[] } {
  if (typeof DOMParser === "undefined" || !html) return { html, sections: [] };
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild;
  if (!root) return { html, sections: [] };
  const sections: Section[] = [];
  const used = new Map<string, number>();
  let current: Section | null = null;
  for (const el of Array.from(root.children)) {
    if (el.tagName === "H2" || el.tagName === "H3") {
      const text = (el.textContent ?? "").trim();
      const base = `s-${slug(text)}`;
      const n = used.get(base) ?? 0;
      used.set(base, n + 1);
      const id = n ? `${base}-${n + 1}` : base;
      el.id = id;
      current = { id, level: el.tagName === "H2" ? 2 : 3, text, reqs: [] };
      sections.push(current);
    } else if (current) {
      for (const m of (el.textContent ?? "").matchAll(/\b(?:FR|SC|NFR)-\d{2,4}\b/g)) {
        if (!current.reqs.includes(m[0])) current.reqs.push(m[0]);
      }
    }
  }
  return { html: root.innerHTML, sections };
}
