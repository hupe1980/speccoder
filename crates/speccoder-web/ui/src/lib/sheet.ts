// A sheet's focus: the first field takes it, Tab stays inside, and when the
// sheet goes the focus returns to where it was (the opener).

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** The elements of `root` a keyboard reaches, in order. */
export function focusable(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => !el.hasAttribute("inert") && el.getAttribute("aria-hidden") !== "true");
}

/** A Svelte action for a sheet: focus in, Tab trapped, focus back out. */
export function trap(node: HTMLElement, first?: string) {
  const opener = document.activeElement as HTMLElement | null;
  const start = () => {
    const want = (first && node.querySelector<HTMLElement>(first)) || node.querySelector<HTMLElement>("[data-first]") || focusable(node)[0];
    want?.focus();
  };
  queueMicrotask(start);
  const key = (e: KeyboardEvent) => {
    if (e.key !== "Tab") return;
    const all = focusable(node);
    if (!all.length) return;
    const at = all.indexOf(document.activeElement as HTMLElement);
    if (e.shiftKey && (at <= 0)) {
      e.preventDefault();
      all[all.length - 1]!.focus();
    } else if (!e.shiftKey && at === all.length - 1) {
      e.preventDefault();
      all[0]!.focus();
    }
  };
  node.addEventListener("keydown", key);
  return {
    destroy() {
      node.removeEventListener("keydown", key);
      if (opener && document.contains(opener)) opener.focus();
    },
  };
}
