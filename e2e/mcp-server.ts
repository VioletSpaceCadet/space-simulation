import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";
import {
  getPage,
  takeScreenshot,
  navigateTo,
  closeBrowser,
} from "./lib/browser.js";

const server = new McpServer({
  name: "playwright-screenshot",
  version: "0.1.0",
});

server.tool(
  "screenshot",
  "Take a screenshot of the running app. Returns a base64 PNG image.",
  {
    path: z.string().optional().describe("URL path (default '/')"),
    width: z.number().int().optional().describe("Viewport width (default 1280)"),
    height: z.number().int().optional().describe("Viewport height (default 720)"),
    fullPage: z.boolean().optional().describe("Capture full page (default false)"),
  },
  async ({ path: urlPath, width, height, fullPage }) => {
    const page = await getPage();
    await navigateTo(page, urlPath ?? "/");
    // Wait for app to hydrate
    await page.waitForTimeout(1000);

    const buffer = await takeScreenshot(page, { width, height, fullPage });

    return {
      content: [
        {
          type: "image" as const,
          data: buffer.toString("base64"),
          mimeType: "image/png",
        },
      ],
    };
  },
);

server.tool(
  "navigate",
  "Navigate to a page in the app and return page info.",
  {
    path: z.string().describe("URL path to navigate to"),
  },
  async ({ path: urlPath }) => {
    const page = await getPage();
    const result = await navigateTo(page, urlPath);

    return {
      content: [
        {
          type: "text" as const,
          text: JSON.stringify(result),
        },
      ],
    };
  },
);

// Cleanup on exit
process.on("exit", () => {
  void closeBrowser();
});

const transport = new StdioServerTransport();
await server.connect(transport);
