// The spec, alive: each user story a card with its priority and how far
// its oracle is; each acceptance scenario its Given / When / Then steps,
// its id and its state — green (and the task that turned it), open (red),
// untested, or not yet written. MUST, SHOULD and MAY stand out.
// Headings keep their text and ids: the outline, links and comments still
// find them.

import type { StoryScenarios } from "./types";

const STORY = /^User Story (\d+)\s*[-–—:]\s*.+?\((Priority:\s*(P\d+))\)/;
const KEYWORDS = /\b(MUST NOT|MUST|SHALL NOT|SHALL|SHOULD NOT|SHOULD|MAY)\b/g;

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] ?? c);

type Mark = StoryScenarios["scenarios"][number];

/** What a scenario's badge says, and its state class. */
export function badge(m: Mark | undefined, written: boolean): { state: string; text: string; title: string } {
  if (!written || !m) return { state: "pending", text: "no test yet", title: "Its test is written before the story's first task" };
  const contract = m.contract ? `Holds: ${m.contract}` : "";
  if (m.state === "green") return { state: "green", text: m.turned_by ? `✓ ${m.turned_by}` : "✓ green", title: [m.turned_by ? `Turned green by ${m.turned_by}` : "Green from the start", contract].filter(Boolean).join("\n") };
  if (m.state === "untested") return { state: "untested", text: "untested", title: "Its author could not test it" };
  return { state: "red", text: "open", title: ["Its test fails: a task of the story is to make it pass", contract].filter(Boolean).join("\n") };
}

/** The story strip: one dot per scenario, `2 of 3`. */
function strip(st: StoryScenarios | undefined): string {
  if (!st) return "";
  const green = st.scenarios.filter((m) => m.state === "green").length;
  const dots = st.scenarios.map((m) => `<i class="d ${st.oracle ? m.state : "pending"}"></i>`).join("");
  const words = st.proven ? "proven" : st.oracle ? `${green} of ${st.scenarios.length} green` : "oracle not yet written";
  return `<span class="strip" title="${esc(words)}">${dots}<span class="n">${esc(words)}</span></span>`;
}

function keywords(root: ParentNode) {
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const hits: Text[] = [];
  for (let n = walk.nextNode(); n; n = walk.nextNode()) {
    const t = n as Text;
    if (t.parentElement?.closest("code, pre")) continue;
    KEYWORDS.lastIndex = 0;
    if (KEYWORDS.test(t.data)) hits.push(t);
  }
  for (const t of hits) {
    const span = document.createElement("span");
    span.innerHTML = esc(t.data).replace(KEYWORDS, '<span class="kw">$1</span>');
    t.replaceWith(...span.childNodes);
  }
}

/** The steps of one scenario's line, when it reads Given … When … Then. */
function steps(li: HTMLLIElement): boolean {
  const h = li.innerHTML.trim();
  if (!/^<strong>Given<\/strong>/.test(h)) return false;
  const parts = h.split(/(?=<strong>(?:Given|When|Then|And|But)<\/strong>)/);
  li.innerHTML = `<span class="steps">${parts
    .map((p) => `<span class="step">${p.replace(/^<strong>(\w+)<\/strong>/, '<b class="k">$1</b>')}</span>`)
    .join("")}</span>`;
  return true;
}

/** `html` (a rendered spec) as the living spec, with `stories` from the report. */
export function living(html: string, stories: StoryScenarios[]): string {
  const t = document.createElement("template");
  t.innerHTML = html;
  const root = t.content;
  const by = new Map(stories.map((s) => [s.story, s]));
  for (const h of Array.from(root.querySelectorAll("h3"))) {
    const m = STORY.exec(h.textContent?.trim() ?? "");
    if (!m) continue;
    const key = `US${m[1]}`;
    const st = by.get(key);
    // The priority as a pill; its words stay for screen readers and search.
    h.innerHTML = h.innerHTML.replace(`(${m[2]})`, `<span class="pri" data-p="${m[3]}">(${m[2]})</span>`);
    const card = document.createElement("section");
    card.className = `story${st?.proven ? " proven" : ""}`;
    card.setAttribute("aria-label", `User Story ${m[1]}`);
    const head = document.createElement("div");
    head.className = "story-head";
    h.before(card);
    let n = h.nextSibling;
    head.append(h);
    head.insertAdjacentHTML("beforeend", strip(st));
    card.append(head);
    while (n && !(n instanceof HTMLElement && /^H[1-3]$/.test(n.tagName))) {
      const next = n.nextSibling;
      card.append(n);
      n = next;
    }
    for (const p of Array.from(card.querySelectorAll(":scope > p"))) {
      const lead = p.querySelector(":scope > strong:first-child")?.textContent?.trim();
      if (lead === "Independent Test") p.classList.add("itest");
      else if (lead === "Why this priority") p.classList.add("why");
      else if (lead === "Acceptance Scenarios") {
        p.classList.add("label");
        const ol = p.nextElementSibling;
        if (ol?.tagName !== "OL") continue;
        ol.classList.add("scenarios");
        Array.from(ol.children).forEach((li, i) => {
          const id = `${key}-${i + 1}`;
          const b = badge(st?.scenarios.find((s) => s.id === id), !!st?.oracle);
          li.classList.add("scenario", b.state);
          steps(li as HTMLLIElement);
          li.insertAdjacentHTML("afterbegin", `<span class="sid">${id}</span>`);
          li.insertAdjacentHTML("beforeend", `<span class="sb" title="${esc(b.title)}">${esc(b.text)}</span>`);
        });
      }
    }
  }
  keywords(root);
  const out = document.createElement("div");
  out.append(root);
  return out.innerHTML;
}
