#!/usr/bin/env node
// Takes the docs' screenshots of the app, light and dark, from the showcase
// (scripts/showcase/: a mock model, the real pipeline): builds it in a temp
// dir with a home and speccoder state of its own, serves it with
// `speccoder ui`, and writes site/static/screenshots/<name>-{light,dark}.webp
// (1200×750 at 2×). Maintainer-run, never in CI. Needs cwebp and cargo-nextest.
//
//   cargo build -p speccoder
//   NODE_PATH=<a folder with playwright installed>/node_modules \
//     node scripts/screenshots.cjs [name…]          # default: every shot below
//
// SHOWCASE=<dir> reuses a showcase setup.sh built there (and keeps it);
// CHROMIUM=<path> uses that browser instead of Playwright's own.
// `speccoder done` names the shots whose components changed after them.

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn, execFileSync } = require("child_process");
const { chromium } = require("playwright");

const root = path.resolve(__dirname, "..");
const out = path.join(root, "site/static/screenshots");
const bin = process.env.SPECCODER_BIN ?? path.join(root, "target/debug/speccoder");
const F = "feature=001-url-slugs";
// Each shot: the page's address in the showcase's url-slugs project.
const SHOTS = {
  inbox: `?${F}`,
  overview: `?${F}&view=overview`,
  board: `?${F}&view=board`,
  drawer: `?${F}&view=board&task=T003`,
  code: `?${F}&view=code`,
  review: `?${F}&view=documents&doc=review`,
  report: `?${F}&view=report`,
  projects: `?${F}&view=projects`,
  thread: `?${F}&view=board&task=T003`,
};

// A shot that needs more than its address: what to do before it is taken.
const POSE = {
  // The attempt's thread in the inspector, its edit open.
  thread: async (page) => {
    const thread = page.locator("section.thread");
    await thread.getByRole("button", { name: /Edited/ }).first().click().catch(() => {});
    await page.locator(".inspector").evaluate((el) => {
      const t = el.querySelector("section.thread");
      if (t) el.scrollTop = t.offsetTop - 110;
    });
    await page.waitForTimeout(600);
  },
};

// The app keeps a live connection open, so the network is never idle: a
// page is settled once no spinner shows and nothing says it is reading,
// for two seconds straight (and at least five after it opened).
async function settled(page, name) {
  const busy = () =>
    page.evaluate(
      () =>
        /Reading the |Loading/.test(document.body.innerText) ||
        [...document.querySelectorAll('[class*="spinner"]')].some((e) => e.offsetParent !== null),
    );
  const start = Date.now();
  let quiet = null;
  while (Date.now() - start < 60_000) {
    if (await busy()) quiet = null;
    else quiet ??= Date.now();
    if (quiet && Date.now() - quiet > 2000 && Date.now() - start > 5000) return;
    await page.waitForTimeout(250);
  }
  throw new Error(`${name} kept loading for a minute`);
}

async function main() {
  const names = process.argv.slice(2);
  for (const n of names) if (!SHOTS[n]) throw new Error(`no shot ${n}: one of ${Object.keys(SHOTS).join(", ")}`);
  const declared = [...fs.readFileSync(path.join(root, "speccoder.toml"), "utf8").matchAll(/^\[screenshots\.([\w-]+)\]/gm)].map((m) => m[1]);
  const unscripted = declared.filter((n) => !SHOTS[n]);
  if (unscripted.length) console.log(`not from the showcase (taken by hand): ${unscripted.join(", ")}`);

  const keep = Boolean(process.env.SHOWCASE);
  const work = process.env.SHOWCASE ? path.resolve(process.env.SHOWCASE) : fs.mkdtempSync(path.join(os.tmpdir(), "showcase-"));
  if (!keep) execFileSync(path.join(__dirname, "showcase/setup.sh"), [work], { stdio: "inherit", env: { ...process.env, SPECCODER_BIN: bin } });
  const home = path.join(work, "home");
  const env = { ...process.env, HOME: home, SPECCODER_STATE_DIR: path.join(work, "state"), OPENAI_API_KEY: "sk-showcase" };
  for (const k of ["XDG_STATE_HOME", "XDG_CONFIG_HOME", "ANTHROPIC_API_KEY"]) delete env[k];

  const ui = spawn(bin, ["ui", "--no-open", "--port", "0"], { cwd: path.join(home, "code/url-slugs"), env, stdio: ["ignore", "pipe", "pipe"] });
  let browser;
  try {
    const url = await new Promise((resolve, reject) => {
      let seen = "";
      const look = (d) => {
        seen += d;
        const m = seen.match(/http:\/\/127\.0\.0\.1:\d+\/#token=[0-9a-f]+/);
        if (m) resolve(m[0]);
      };
      ui.stdout.on("data", look);
      ui.stderr.on("data", look);
      ui.on("exit", (c) => reject(new Error(`speccoder ui exited (${c}): ${seen}`)));
      setTimeout(() => reject(new Error(`speccoder ui printed no address: ${seen}`)), 30_000);
    });
    const [base, token] = url.split("#");
    browser = await chromium.launch(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {});
    const pngs = fs.mkdtempSync(path.join(os.tmpdir(), "shots-"));
    for (const scheme of ["light", "dark"]) {
      const page = await browser.newPage({ viewport: { width: 1200, height: 750 }, deviceScaleFactor: 2, colorScheme: scheme, reducedMotion: "reduce" });
      for (const name of names.length ? names : Object.keys(SHOTS)) {
        await page.goto(`${base}${SHOTS[name]}#${token}`);
        await settled(page, name);
        if (POSE[name]) await POSE[name](page);
        const png = path.join(pngs, `${name}-${scheme}.png`);
        await page.screenshot({ path: png });
        const webp = path.join(out, `${name}-${scheme}.webp`);
        execFileSync("cwebp", ["-quiet", "-q", "80", "-m", "6", png, "-o", webp]);
        console.log(`wrote ${path.relative(root, webp)}`);
      }
      await page.close();
    }
    fs.rmSync(pngs, { recursive: true, force: true });
  } finally {
    if (browser) await browser.close();
    ui.kill();
    if (!keep) fs.rmSync(work, { recursive: true, force: true });
  }
}

main().catch((e) => {
  console.error(e.message ?? e);
  process.exit(1);
});
