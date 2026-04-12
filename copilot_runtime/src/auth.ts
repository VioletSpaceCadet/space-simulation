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

import { timingSafeEqual } from "node:crypto";

import type { RequestHandler } from "express";

export const SHARED_SECRET_HEADER = "x-copilot-runtime-secret";

/**
 * Near-constant-time string comparison via `node:crypto`. Node's native
 * `timingSafeEqual` is guaranteed constant-time over equal-length buffers
 * and cannot be JIT-optimized into an early-exit loop the way a hand-rolled
 * comparison can.
 *
 * Caveat: `Buffer.copy` is O(min(provided, expected)), so a provided value
 * far shorter than the expected secret runs slightly faster than one that
 * matches the expected length. This leaks the secret's length order of
 * magnitude to a local attacker who can measure thousands of responses.
 * That's acceptable for the threat model (localhost hardening against
 * other processes on the same machine) — a network-facing auth layer
 * would need a different primitive.
 */
function safeEqual(provided: string, expected: string): boolean {
  const providedBuf = Buffer.from(provided, "utf8");
  const expectedBuf = Buffer.from(expected, "utf8");

  // `timingSafeEqual` throws on length mismatch, so pad the provided buffer
  // to the expected length and compare bytes, then AND with a separate
  // length equality check. Both branches run unconditionally so the
  // control flow does not short-circuit on length mismatch.
  const padded = Buffer.alloc(expectedBuf.length);
  providedBuf.copy(padded);
  const bytesEqual = timingSafeEqual(padded, expectedBuf);
  const lengthsEqual = providedBuf.length === expectedBuf.length;
  return bytesEqual && lengthsEqual;
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
