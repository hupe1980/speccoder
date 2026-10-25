<script lang="ts" module>
  // Symbols drawn in one weight on a 24-point grid, after SF Symbols: round
  // caps and joins, strokes in the current colour, a few filled marks.
  // Decorative unless given a label.
  const paths: Record<string, string> = {
    check: "M5 12.5l4.2 4.2L19 7",
    seal: "M12 2.8l2.3 1.7 2.8-.2.9 2.7 2.4 1.5-.7 2.8.7 2.8-2.4 1.5-.9 2.7-2.8-.2L12 21.2l-2.3-1.7-2.8.2-.9-2.7-2.4-1.5.7-2.8-.7-2.8 2.4-1.5.9-2.7 2.8.2z M8.6 12.2l2.3 2.3 4.6-4.8",
    loop: "M4.5 12a7.5 7.5 0 0 1 12.9-5.2L20 9.3 M20 4.5v4.8h-4.8 M19.5 12a7.5 7.5 0 0 1-12.9 5.2L4 14.7 M4 19.5v-4.8h4.8",
    signature: "M3.5 17.5c2.5-1 4.5-5 5.5-8.5.6-2.2-1.6-2.4-2.1-.3-.8 3.2.3 7.6 2.6 7.8 1.7.1 2.2-2.6 3.5-2.6 1.1 0 .9 2.2 2.2 2.2 1 0 1.7-1.2 2.6-1.2 M3.5 20.5h17",
    exclaim: "M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17z M12 7.8v5 M12 16.2v.1",
    skip: "M5 6.5l7 5.5-7 5.5z M12.5 6.5l7 5.5-7 5.5z",
    circle: "M12 4.5a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15z",
    hourglass: "M7 3.5h10 M7 20.5h10 M8 3.5c0 4.5 4 5.2 4 8.5s-4 4-4 8.5 M16 3.5c0 4.5-4 5.2-4 8.5s4 4 4 8.5",
    hand: "M8.5 12.5V5.8a1.4 1.4 0 0 1 2.8 0v5.7 M11.3 11V4.6a1.4 1.4 0 0 1 2.8 0V11 M14.1 11V5.8a1.4 1.4 0 0 1 2.8 0v7.4 M8.5 12.5 7.2 11a1.5 1.5 0 0 0-2.3 1.9l3.3 4.8a6 6 0 0 0 5 2.8h.4a4.6 4.6 0 0 0 4.6-4.6v-3.2",
    xmark: "M6.5 6.5l11 11 M17.5 6.5l-11 11",
    xoctagon: "M8.4 3.5h7.2l4.9 4.9v7.2l-4.9 4.9H8.4l-4.9-4.9V8.4z M9.2 9.2l5.6 5.6 M14.8 9.2l-5.6 5.6",
    play: "M7.5 5.2v13.6a.8.8 0 0 0 1.2.7l10.7-6.8a.8.8 0 0 0 0-1.4L8.7 4.5a.8.8 0 0 0-1.2.7z",
    stop: "M7 6.5h10a.5.5 0 0 1 .5.5v10a.5.5 0 0 1-.5.5H7a.5.5 0 0 1-.5-.5V7a.5.5 0 0 1 .5-.5z",
    grid: "M4.5 4.5h6v6h-6z M13.5 4.5h6v6h-6z M4.5 13.5h6v6h-6z M13.5 13.5h6v6h-6z",
    doc: "M7 3.5h6.5l4.5 4.5v12.5H7z M13.5 3.5V8H18 M9.5 12h6 M9.5 15h6 M9.5 18h4",
    chart: "M5 19.5V13 M10 19.5V8 M15 19.5v-9 M20 19.5V5 M3.5 19.5h18",
    bell: "M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z M10 20.5a2 2 0 0 0 4 0",
    search: "M10.5 4a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13z M15.3 15.3 20 20",
    command: "M9 9V6.5A2.5 2.5 0 1 0 6.5 9H9zm0 0v6m0-6h6m-6 6v2.5A2.5 2.5 0 1 1 6.5 15H9zm0 0h6m0-6V6.5A2.5 2.5 0 1 1 17.5 9H15zm0 0v6m0 0v2.5a2.5 2.5 0 1 0 2.5-2.5H15z",
    sun: "M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8z M12 2.5v2 M12 19.5v2 M2.5 12h2 M19.5 12h2 M5.3 5.3l1.4 1.4 M17.3 17.3l1.4 1.4 M5.3 18.7l1.4-1.4 M17.3 6.7l1.4-1.4",
    moon: "M19.5 14.5A8 8 0 0 1 9.5 4.5a8 8 0 1 0 10 10z",
    halfcircle: "M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17z M12 3.5v17",
    copy: "M8.5 8.5h10v11h-10z M15.5 8.5V4.5h-10v11h3",
    chevron: "M9.5 6l6 6-6 6",
    down: "M6 9.5l6 6 6-6",
    key: "M8.5 10a4 4 0 1 1 3.9 3.1L20 13v3h-2.5v2.5H15V16l-1.6-1.6 M7.5 10.5v.1",
    tray: "M4 13.5 6.5 5h11l2.5 8.5v5H4z M4 13.5h4.5a3.5 3.5 0 0 0 7 0H20",
    terminal: "M4 5h16v14H4z M7.5 9.5l3 2.5-3 2.5 M12.5 15h4",
    sparkle: "M12 3.5c.6 4.2 2.8 6.4 7 7-4.2.6-6.4 2.8-7 7-.6-4.2-2.8-6.4-7-7 4.2-.6 6.4-2.8 7-7z",
    branch: "M7 4.5v15 M7 4.5a2 2 0 1 1 0 .1 M7 19.5a2 2 0 1 1 0 .1 M17 8.5a2 2 0 1 1 0 .1 M17 10.5c0 4-10 3-10 7",
    shield: "M12 3.5 19 6v5.5c0 4.4-3 7.7-7 9-4-1.3-7-4.6-7-9V6z M9 12l2.2 2.2L15.5 10",
    wifi: "M12 18.5v.1 M8.6 15.2a5 5 0 0 1 6.8 0 M5.6 12.2a9.2 9.2 0 0 1 12.8 0",
    list: "M9 6.5h11 M9 12h11 M9 17.5h11 M4.5 6.5v.1 M4.5 12v.1 M4.5 17.5v.1",
    arrowup: "M12 19.5v-15 M6 10.5l6-6 6 6",
    gear: "M12 9a3 3 0 1 1 0 6 3 3 0 0 1 0-6z M12 2.8l1.6 2.1 2.6-.5.8 2.5 2.4 1.2-.6 2.6 1.6 2.1-2.1 1.6.5 2.6-2.5.8-1.2 2.4-2.6-.6-2.1 1.6-1.6-2.1-2.6.5-.8-2.5-2.4-1.2.6-2.6-1.6-2.1 2.1-1.6-.5-2.6 2.5-.8 1.2-2.4 2.6.6z",
    cpu: "M7 7h10v10H7z M10 10h4v4h-4z M9.5 3.5v3.5 M14.5 3.5v3.5 M9.5 17v3.5 M14.5 17v3.5 M3.5 9.5H7 M3.5 14.5H7 M17 9.5h3.5 M17 14.5h3.5",
    gauge: "M4.5 16.5a8 8 0 1 1 15 0 M12 13.5l3.5-4.5 M12 13.5v.1",
    lock: "M7 11h10v9H7z M9 11V8a3 3 0 0 1 6 0v3",
    plus: "M12 5v14 M5 12h14",
    minus: "M5 12h14",
    undo: "M9 7 4.5 11.5 9 16 M5 11.5h9a5 5 0 0 1 0 10h-2",
    dollar: "M12 3.5v17 M16 7.5c-.6-1.5-2.2-2.3-4-2.3-2.4 0-4 1.2-4 3s1.6 2.6 4 3.2 4 1.4 4 3.3-1.6 3.1-4 3.1c-1.9 0-3.5-.9-4.1-2.5",
    info: "M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17z M12 11v5.5 M12 7.8v.1",
  };
  const filled = new Set(["play", "stop"]);
  export type IconName = keyof typeof paths;
</script>

<script lang="ts">
  let { name, size = 16, label, weight = 1.8 }: { name: string; size?: number; label?: string; weight?: number } = $props();
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={filled.has(name) ? "currentColor" : "none"}
  stroke="currentColor"
  stroke-width={filled.has(name) ? 0 : weight}
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
><path d={paths[name] ?? paths.circle} /></svg>

<style>
  .icon { flex: none; display: inline-block; vertical-align: -0.18em; }
</style>
