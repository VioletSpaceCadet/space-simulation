/**
 * copilot_runtime sidecar entrypoint.
 *
 * Boots an Express server on 127.0.0.1:4000 and exposes CopilotKit's runtime
 * endpoint at `/api/copilotkit`. Ships alongside sim_daemon and ui_web for the
 * local development stack:
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

import express from "express";
import cors from "cors";
import {
  copilotRuntimeNodeExpressEndpoint,
  ExperimentalEmptyAdapter,
} from "@copilotkit/runtime";
import { buildAdapterFromEnv } from "./adapter.js";
import { getSharedSecret } from "./credentials.js";
import { buildRuntime } from "./runtime.js";
import { createSharedSecretMiddleware } from "./auth.js";

const HOST = "127.0.0.1";
const PORT = Number(process.env.COPILOT_RUNTIME_PORT ?? 4000);
const COPILOT_ENDPOINT = "/api/copilotkit";
const UI_ORIGIN = process.env.COPILOT_UI_ORIGIN ?? "http://localhost:5173";

function main(): void {
  // Resolve all secrets + adapters up front. Any failure here should crash the
  // process with a clear message before we accept any requests.
  const adapter = buildAdapterFromEnv();
  const sharedSecret = getSharedSecret();
  const runtime = buildRuntime(adapter);

  const copilotHandler = copilotRuntimeNodeExpressEndpoint({
    endpoint: COPILOT_ENDPOINT,
    runtime,
    // BuiltInAgent handles model invocation; the empty adapter is the
    // canonical no-op service adapter for the v2 agent path.
    serviceAdapter: new ExperimentalEmptyAdapter(),
  });

  const app = express();

  // Explicit CORS: only the ui_web dev server may talk to us. No wildcard
  // origin. Credentials disabled — the shared-secret header is the gate.
  app.use(
    cors({
      origin: UI_ORIGIN,
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
  // requests must not reach the CopilotKit handler. The handler is wrapped so
  // its Promise-returning behavior doesn't trip `no-misused-promises`;
  // CopilotKit writes its own response and handles errors internally, so we
  // intentionally discard the returned promise.
  app.use(
    COPILOT_ENDPOINT,
    createSharedSecretMiddleware(sharedSecret),
    (req, res) => {
      void copilotHandler(req, res);
    },
  );

  app.listen(PORT, HOST, () => {
    // Startup log is the user's only confirmation that the sidecar is live.
    console.log(
      `copilot_runtime listening on http://${HOST}:${PORT}${COPILOT_ENDPOINT} ` +
      `(provider=${adapter.provider}, model=${adapter.model}, origin=${UI_ORIGIN})`,
    );
  });
}

// Only run when invoked directly. Importing `index.ts` in tests should not
// start the server.
const isDirectInvocation = import.meta.url === `file://${process.argv[1]}`;
if (isDirectInvocation) {
  try {
    main();
  } catch (err) {
    // Fatal startup error must print the install instruction for the user.
    console.error("copilot_runtime failed to start:", err instanceof Error ? err.message : err);
    process.exit(1);
  }
}
