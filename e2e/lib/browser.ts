import { chromium, type Browser, type Page } from "playwright";

const BASE_URL = process.env.BASE_URL ?? "http://localhost:5173";

let browser: Browser | null = null;
let page: Page | null = null;

export async function launchBrowser(): Promise<Browser> {
  if (browser) return browser;
  browser = await chromium.launch({ headless: true });
  return browser;
}

export async function getPage(): Promise<Page> {
  if (page && !page.isClosed()) return page;
  const b = await launchBrowser();
  const context = await b.newContext({
    viewport: { width: 1280, height: 720 },
  });
  page = await context.newPage();
  return page;
}

export interface ScreenshotOptions {
  path?: string;
  width?: number;
  height?: number;
  fullPage?: boolean;
}

export async function takeScreenshot(
  targetPage: Page,
  options: ScreenshotOptions = {},
): Promise<Buffer> {
  const {
    width = 1280,
    height = 720,
    fullPage = false,
  } = options;
  await targetPage.setViewportSize({ width, height });
  return targetPage.screenshot({ fullPage, type: "png" });
}

export async function navigateTo(
  targetPage: Page,
  urlPath: string,
): Promise<{ title: string; url: string; success: boolean }> {
  const url = urlPath.startsWith("http")
    ? urlPath
    : `${BASE_URL}${urlPath.startsWith("/") ? urlPath : `/${urlPath}`}`;
  try {
    await targetPage.goto(url, { waitUntil: "networkidle" });
    return {
      title: await targetPage.title(),
      url: targetPage.url(),
      success: true,
    };
  } catch {
    return {
      title: "",
      url,
      success: false,
    };
  }
}

export async function closeBrowser(): Promise<void> {
  if (browser) {
    await browser.close();
    browser = null;
    page = null;
  }
}
