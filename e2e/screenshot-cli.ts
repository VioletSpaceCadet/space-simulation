import fs from "node:fs";
import {
  getPage,
  takeScreenshot,
  navigateTo,
  closeBrowser,
} from "./lib/browser.js";

const args = process.argv.slice(2);
const urlPath = args[0] ?? "/";
const outputIndex = args.indexOf("--output");
const output = outputIndex >= 0 ? args[outputIndex + 1] : "./screenshot.png";
const widthIndex = args.indexOf("--width");
const width = widthIndex >= 0 ? parseInt(args[widthIndex + 1], 10) : 1280;
const heightIndex = args.indexOf("--height");
const height = heightIndex >= 0 ? parseInt(args[heightIndex + 1], 10) : 720;

const page = await getPage();
await navigateTo(page, urlPath);
await page.waitForTimeout(1000);
const buffer = await takeScreenshot(page, { width, height });
fs.writeFileSync(output, buffer);
console.log(`Screenshot saved to ${output}`);
await closeBrowser();
