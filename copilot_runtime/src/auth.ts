/**
 * Localhost hardening: shared-secret header check for copilot_runtime.
 *
 * Guards the CopilotKit runtime endpoint against other local processes hitting
 * it directly. The secret is pre-shared between this sidecar and ui_web;
 * requests without a matching `X-Copilot-Runtime-Secret` header are rejected
 * with 401.
 *
 * Threat model: this is localhost-only hardening, not a general auth system.
 * The runtime binds to 127.0.0.1 so off-host traffic is impossible; the header
 * check prevents unauthorized local processes (e.g. a malicious npm script)
 * from silently driving the LLM backend.
 */

import type { RequestHandler } from "express";

export const SHARED_SECRET_HEADER = "x-copilot-runtime-secret";

/**
 * Constant-time string comparison to avoid timing oracles leaking the secret.
 * Falls back to a constant-false path when lengths differ, so even early-exit
 * on length comparison doesn't tip off an attacker.
 */
function safeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) { return false; }
  let mismatch = 0;
  for (let i = 0; i < a.length; i++) {
    mismatch |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return mismatch === 0;
}

/**
 * Express middleware that requires a matching shared secret in the
 * `X-Copilot-Runtime-Secret` request header. Designed to mount BEFORE the
 * CopilotKit runtime endpoint so unauthenticated calls never reach the LLM.
 *
 * CORS preflight (`OPTIONS`) requests pass through unauthenticated — browsers
 * do not send custom headers on preflight, so blocking preflight would break
 * the frontend entirely. The subsequent actual request still requires the
 * header.
 */
export function createSharedSecretMiddleware(expectedSecret: string): RequestHandler {
  if (expectedSecret.length === 0) {
    throw new Error(
      "copilot_runtime: createSharedSecretMiddleware called with empty secret. " +
      "Refusing to start — this would disable the localhost hardening check.",
    );
  }

  return (req, res, next) => {
    if (req.method === "OPTIONS") {
      next();
      return;
    }

    const provided = req.header(SHARED_SECRET_HEADER);
    if (typeof provided !== "string" || !safeEqual(provided, expectedSecret)) {
      res.status(401).json({
        error: "missing_or_invalid_shared_secret",
        hint:
          `Set the ${SHARED_SECRET_HEADER} header to the value stored in your ` +
          "Keychain / COPILOT_RUNTIME_SECRET env var. See copilot_runtime/README.md.",
      });
      return;
    }

    next();
  };
}
