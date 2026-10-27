// Shared by the component tests: find what a person sees, type, let the
// page settle, and check the rendered page with axe (the WCAG rules a DOM
// stand-in can judge: names, roles, ARIA, labels — not contrast or layout).

import { flushSync } from "svelte";
import axe from "axe-core";

export function button(name: string | RegExp): HTMLButtonElement {
  const all = [...document.querySelectorAll("button")];
  const b = all.find((x) => (typeof name === "string" ? x.textContent?.trim() === name || x.getAttribute("aria-label") === name : name.test(x.textContent ?? "")));
  if (!b) throw new Error(`no button "${name}" in ${document.body.innerHTML}`);
  return b;
}

export function type(el: HTMLInputElement | HTMLTextAreaElement, value: string) {
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

export function press(key: string, mods: KeyboardEventInit = {}, target: EventTarget = document.activeElement ?? document.body) {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...mods }));
  flushSync();
}

export const settle = async (n = 6) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
  flushSync();
};

/** The axe violations of the rendered page (contrast and layout rules
 *  are off: a DOM stand-in draws nothing). */
export async function violations(root: Element = document.body): Promise<string[]> {
  // axe schedules its work with timers: real ones.
  const { vi } = await import("vitest");
  vi.useRealTimers();
  const r = await axe.run(root as HTMLElement, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false }, "landmark-one-main": { enabled: false }, "page-has-heading-one": { enabled: false } },
    resultTypes: ["violations"],
  });
  return r.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}
