/**
 * copilot_runtime sidecar entrypoint.
 *
 * Boots an Express server on 127.0.0.1:4000 and exposes CopilotKit's v2
 * runtime endpoint at `/api/copilotkit`. Ships alongside sim_daemon and
 * ui_web for the local development stack:
 *
 *   sim_daemon      → :3001   (HTTP + SSE, game loop)
 *   ui_web (vite)   → :5173   (React mission control)
 *   copilot_runtime → :4000   (this process)
 *   ollama (Phase B)→ :11434  (local inference)
 *
 * The runtime binds to loopback only. The shared-secret header check defends
 * against other local processes on the same host; see auth.ts for the threat
 * model.
 */

import { pathToFileURL } from "node:url";
import express from "express";
import cors from "cors";
import { createCopilotExpressHandler } from "@copilotkit/runtime/v2/express";
import { buildAdapterFromEnv } from "./adapter.js";
import { getSharedSecret } from "./credentials.js";
import { buildRuntime } from "./runtime.js";
import { createSharedSecretMiddleware } from "./auth.js";

const HOST = "127.0.0.1";
const DEFAULT_PORT = 4000;
const COPILOT_ENDPOINT = "/api/copilotkit";
const DEFAULT_UI_ORIGIN = "http://localhost:5173";

function resolvePort(raw: string | undefined): number {
  const parsed = Number(raw ?? DEFAULT_PORT);
  if (!Number.isInteger(parsed) || parsed <= 0 || parsed > 65535) {
    throw new Error(
      `copilot_runtime: invalid COPILOT_RUNTIME_PORT="${raw}". ` +
      "Expected an integer in 1..65535.",
    );
  }
  return parsed;
}

function resolveUiOrigin(raw: string | undefined): string {
  const candidate = raw?.trim();
  if (candidate && candidate.length > 0) { return candidate; }
  return DEFAULT_UI_ORIGIN;
}

function main(): void {
  // Resolve all secrets + adapters up front. Any failure here should crash the
  // process with a clear message before we accept any requests.
  const adapter = buildAdapterFromEnv();
  const sharedSecret = getSharedSecret();
  const runtime = buildRuntime(adapter);
  const port = resolvePort(process.env.COPILOT_RUNTIME_PORT);
  const uiOrigin = resolveUiOrigin(process.env.COPILOT_UI_ORIGIN);

  // CopilotKit ships its own cors middleware when configured, but we want a
  // single known CORS policy mounted in front of everything (including the
  // health check), so we pass `cors: false` on the CopilotKit side and run
  // our own `cors()` first.
  const copilotRouter = createCopilotExpressHandler({
    runtime,
    basePath: COPILOT_ENDPOINT,
    cors: false,
  });

  const app = express();

  // Explicit CORS: only the ui_web dev server may talk to us. No wildcard
  // origin. Credentials disabled — the shared-secret header is the gate.
  app.use(
    cors({
      origin: uiOrigin,
      methods: ["GET", "POST", "OPTIONS"],
      allowedHeaders: ["Content-Type", "X-Copilot-Runtime-Secret"],
      credentials: false,
    }),
  );

  // Health check for the ui_web "is the sidecar up?" indicator. Intentionally
  // does NOT require the shared secret so ui_web can probe before showing the
  // chat UI; leaks no state.
  app.get("/healthz", (_req, res) => {
    res.json({ status: "ok", provider: adapter.provider, model: adapter.model });
  });

  // Shared-secret gate, then CopilotKit runtime. Order matters — unauthorized
  // requests must not reach the CopilotKit handler.
  app.use(COPILOT_ENDPOINT, createSharedSecretMiddleware(sharedSecret));
  app.use(copilotRouter);

  const server = app.listen(port, HOST, () => {
    // Startup log is the user's only confirmation that the sidecar is live.
    console.log(
      `copilot_runtime listening on http://${HOST}:${port}${COPILOT_ENDPOINT} ` +
      `(provider=${adapter.provider}, model=${adapter.model}, origin=${uiOrigin})`,
    );
  });

  server.on("error", (err: NodeJS.ErrnoException) => {
    if (err.code === "EADDRINUSE") {
      console.error(
        `copilot_runtime: port ${port} is already in use. ` +
        "Stop the other process or set COPILOT_RUNTIME_PORT to a free port.",
      );
    } else {
      console.error("copilot_runtime server error:", err);
    }
    process.exit(1);
  });
}

// Only run when invoked directly. Importing `index.ts` in tests should not
// start the server. `pathToFileURL` handles spaces and symlinks correctly on
// macOS — a raw `file://${argv[1]}` comparison silently breaks on
// `/private/var` ↔ `/var` and URL-encoded paths.
const isDirectInvocation =
  typeof process.argv[1] === "string" &&
  import.meta.url === pathToFileURL(process.argv[1]).href;

if (isDirectInvocation) {
  try {
    main();
  } catch (err) {
    // Fatal startup error must print the install instruction for the user.
    console.error("copilot_runtime failed to start:", err instanceof Error ? err.message : err);
    process.exit(1);
  }
}
