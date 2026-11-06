#!/usr/bin/env node
// Records the sample's tour as a video: the video is the tour, so the two
// never drift apart. Maintainer-run, never in CI.
//
//   speccoder sample
//   speccoder ui -C <the path it prints> --port 7417 --no-open   # prints the URL with its token
//   NODE_PATH=<a folder with playwright installed>/node_modules \
//     node scripts/record-tour.cjs '<that URL>' site/static/tour.mp4
//
// CHROMIUM=<path> uses that browser instead of Playwright's own. An `.mp4`
// output (H.264, what every browser and QuickTime play) needs ffmpeg.

const fs = require("fs");
const path = require("path");
const { chromium } = require("playwright");

async function main() {
  const [url, out = "tour.mp4"] = process.argv.slice(2);
  if (!url) {
    console.error("usage: node scripts/record-tour.cjs <url with #token=…> [out.mp4|out.webm]");
    process.exit(2);
  }
  const dir = fs.mkdtempSync(path.join(require("os").tmpdir(), "tour-"));
  const browser = await chromium.launch(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {});
  const size = { width: 1280, height: 800 };
  const page = await browser.newPage({ viewport: size, colorScheme: "light", recordVideo: { dir, size } });
  await page.goto(url);
  await page.getByRole("button", { name: /Take the tour/ }).click();
  // The end card is the tour's last frame.
  await page.getByText("Start your own").waitFor({ timeout: 120_000 });
  await page.waitForTimeout(2500);
  const video = page.video();
  await page.close();
  await browser.close();
  const recorded = await video.path();
  if (out.endsWith(".mp4")) {
    require("child_process").execFileSync("ffmpeg", ["-v", "error", "-y", "-i", recorded, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-crf", "24", "-an", out], { stdio: "inherit" });
  } else {
    fs.copyFileSync(recorded, out);
  }
  console.log(`wrote ${out}`);
}

main().catch((e) => {
  console.error(e.message ?? e);
  process.exit(1);
});
