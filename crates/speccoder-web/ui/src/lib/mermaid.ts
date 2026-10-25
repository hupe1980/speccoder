// Mermaid diagrams in rendered Markdown: a ```mermaid block becomes its
// diagram. Mermaid is loaded only when a page shows one (from the binary's
// own bundle — nothing from the network), runs with `securityLevel:
// "strict"` (no HTML labels, no click handlers), and follows the theme. A
// diagram that does not parse keeps its source, with the error beside it.

let loaded: Promise<typeof import("mermaid").default> | null = null;
let count = 0;

function dark(): boolean {
  const t = document.documentElement.getAttribute("data-theme");
  return t ? t === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
}

async function engine() {
  loaded ??= import("mermaid").then((m) => m.default);
  const mermaid = await loaded;
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: dark() ? "dark" : "neutral",
    fontFamily: "-apple-system, BlinkMacSystemFont, 'SF Pro Text', system-ui, sans-serif",
  });
  return mermaid;
}

async function renderIn(root: HTMLElement) {
  const blocks = [...root.querySelectorAll<HTMLElement>("pre > code.language-mermaid")];
  if (!blocks.length) return;
  const mermaid = await engine();
  for (const code of blocks) {
    const pre = code.parentElement!;
    const source = code.textContent ?? "";
    const figure = document.createElement("figure");
    figure.className = "diagram";
    try {
      const { svg } = await mermaid.render(`mmd-${++count}`, source);
      figure.innerHTML = svg; // Mermaid's own sanitised output (strict mode)
      figure.title = "Mermaid diagram";
    } catch (e) {
      const err = document.createElement("p");
      err.className = "diagram-error";
      err.textContent = `This diagram does not parse: ${String((e as Error).message ?? e).split("\n")[0]}`;
      figure.append(pre.cloneNode(true), err);
    }
    pre.replaceWith(figure);
  }
}

/** A Svelte action: renders the diagrams in `node`, again when `key` changes. */
export function diagrams(node: HTMLElement, key?: unknown) {
  void renderIn(node);
  return {
    update(_: unknown) {
      void renderIn(node);
    },
  };
}
