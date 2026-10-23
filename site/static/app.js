// speccoder — theme toggle, copy buttons, search. No dependencies beyond Zola's elasticlunr.
(() => {
  "use strict";
  const root = document.documentElement;

  // Theme: light → dark → system, remembered per reader.
  const toggle = document.querySelector("[data-theme-toggle]");
  const systemDark = () => matchMedia("(prefers-color-scheme: dark)").matches;
  const current = () => root.dataset.theme || (systemDark() ? "dark" : "light");
  toggle?.addEventListener("click", () => {
    const next = current() === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    try { localStorage.setItem("theme", next); } catch (e) {}
  });

  // Docs menu: open beside the page, collapsed above it on small screens.
  const menu = document.querySelector("[data-docs-menu]");
  if (menu && matchMedia("(max-width: 860px)").matches) menu.open = false;

  // Copy: explicit [data-copy] buttons, and one on every code block in the docs.
  const copy = async (button, text) => {
    try {
      await navigator.clipboard.writeText(text);
      const label = button.textContent;
      button.textContent = "Copied";
      setTimeout(() => { button.textContent = label; }, 1400);
    } catch (e) { button.textContent = "Press ⌘C"; }
  };
  document.querySelectorAll("[data-copy]").forEach((b) =>
    b.addEventListener("click", () => copy(b, b.dataset.copy)));
  document.querySelectorAll(".prose pre").forEach((pre) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "copy";
    b.textContent = "Copy";
    b.setAttribute("aria-label", "Copy code");
    b.addEventListener("click", () => copy(b, pre.querySelector("code")?.innerText ?? pre.innerText));
    pre.append(b);
  });

  // Table of contents: mark the heading being read.
  const tocLinks = [...document.querySelectorAll(".toc a")];
  if (tocLinks.length && "IntersectionObserver" in window) {
    const byId = new Map(tocLinks.map((a) => [decodeURIComponent(a.hash.slice(1)), a]));
    const seen = new Set();
    const io = new IntersectionObserver((entries) => {
      entries.forEach((e) => (e.isIntersecting ? seen.add(e.target.id) : seen.delete(e.target.id)));
      const first = [...byId.keys()].find((id) => seen.has(id));
      if (first) tocLinks.forEach((a) => a.classList.toggle("current", a === byId.get(first)));
    }, { rootMargin: "-80px 0px -65% 0px" });
    byId.forEach((_, id) => { const h = document.getElementById(id); if (h) io.observe(h); });
  }

  // Search: Zola's elasticlunr index, opened with "/" or the header button.
  const dialog = document.querySelector("[data-search]");
  if (!dialog) return;
  const input = dialog.querySelector("#search-input");
  const list = dialog.querySelector(".search-results");
  let index = null;
  let selected = -1;

  const load = () => {
    if (!index && window.elasticlunr && window.searchIndex) index = elasticlunr.Index.load(window.searchIndex);
    return index;
  };
  const open = () => { dialog.hidden = false; input.value = ""; render([]); input.focus(); };
  const close = () => { dialog.hidden = true; };
  const escape = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
  const snippet = (body, terms) => {
    const lower = body.toLowerCase();
    const at = Math.max(0, Math.min(...terms.map((t) => lower.indexOf(t)).filter((i) => i >= 0), 0) - 40);
    const text = body.slice(at, at + 160).trim();
    return (at > 0 ? "…" : "") + escape(text) + (body.length > at + 160 ? "…" : "");
  };
  const render = (results, query = "") => {
    selected = results.length ? 0 : -1;
    if (!query) { list.innerHTML = ""; return; }
    if (!results.length) { list.innerHTML = `<li class="empty">No results for “${escape(query)}”.</li>`; return; }
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    list.innerHTML = results.slice(0, 10).map((r, i) =>
      `<li role="option" aria-selected="${i === 0}"><a href="${r.ref}">${escape(r.doc.title || r.ref)}<small>${snippet(r.doc.body || "", terms)}</small></a></li>`).join("");
  };
  const move = (step) => {
    const items = [...list.querySelectorAll("li[role=option]")];
    if (!items.length) return;
    selected = (selected + step + items.length) % items.length;
    items.forEach((li, i) => li.setAttribute("aria-selected", String(i === selected)));
    items[selected].scrollIntoView({ block: "nearest" });
  };

  input.addEventListener("input", () => {
    const q = input.value.trim();
    const idx = load();
    if (!idx) return render([], q);
    render(q ? idx.search(q, { bool: "AND", expand: true, fields: { title: { boost: 3 }, body: { boost: 1 } } }) : [], q);
  });
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); move(1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(-1); }
    else if (e.key === "Enter") {
      const a = list.querySelectorAll("li[role=option] a")[selected];
      if (a) { e.preventDefault(); location.href = a.href; }
    }
  });
  dialog.addEventListener("click", (e) => { if (e.target === dialog) close(); });
  document.querySelectorAll("[data-search-open]").forEach((b) => b.addEventListener("click", open));
  document.addEventListener("keydown", (e) => {
    const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName) || document.activeElement?.isContentEditable;
    if (e.key === "Escape" && !dialog.hidden) close();
    else if ((e.key === "/" && !typing) || (e.key === "k" && (e.metaKey || e.ctrlKey))) { e.preventDefault(); open(); }
  });
})();
