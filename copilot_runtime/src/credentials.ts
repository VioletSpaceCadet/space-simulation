/**
 * macOS Keychain credential retrieval for copilot_runtime.
 *
 * Plan decision 13: cloud-provider API keys and localhost shared secrets live in
 * the macOS Keychain, not in .env files, so they never leak into shell history,
 * process environment dumps, or accidental commits.
 *
 * Rules:
 * - Read once at startup, cache in module scope. Never re-read per request.
 * - Fail loudly with install instructions if the keychain entry is missing.
 * - macOS-only by design. If portability becomes relevant later, wrap with
 *   platform detection at that point — do not prematurely abstract.
 */

import { execFileSync } from "node:child_process";

const DEFAULT_ACCOUNT = "copilot_runtime";
const OPENROUTER_SERVICE = "OPENROUTER_API_KEY";
const SHARED_SECRET_SERVICE = "COPILOT_RUNTIME_SECRET";

/**
 * Fetches a secret from the macOS Keychain via `security(1)`.
 *
 * Uses `execFileSync` (not `execSync`) so arguments are passed as a list and
 * cannot be interpreted as shell metacharacters. Callers are trusted, but we
 * defend anyway.
 */
export function readKeychainSecret(account: string, service: string): string {
  try {
    const raw = execFileSync(
      "security",
      ["find-generic-password", "-a", account, "-s", service, "-w"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
    return raw.trim();
  } catch (err) {
    const hint =
      `security add-generic-password -a "${account}" -s "${service}" -w "<value>"`;
    throw new Error(
      `copilot_runtime: missing Keychain entry for account="${account}" service="${service}".\n` +
      `Install with:\n  ${hint}\n` +
      `Underlying error: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
}

// --- Module-scoped caches (read-once-at-startup contract) ---

let cachedOpenRouterKey: string | null = null;
let cachedSharedSecret: string | null = null;

/**
 * Returns the OpenRouter API key. Reads from Keychain on first call; cached
 * thereafter. Throws with install instructions if missing.
 */
export function getOpenRouterKey(): string {
  cachedOpenRouterKey ??= readKeychainSecret(DEFAULT_ACCOUNT, OPENROUTER_SERVICE);
  return cachedOpenRouterKey;
}

/**
 * Returns the localhost shared-secret used to authenticate CopilotKit runtime
 * requests from `ui_web`. Env var `COPILOT_RUNTIME_SECRET` takes precedence
 * (useful for tests and CI), otherwise reads from Keychain.
 *
 * Falls back to env-var-only because `ui_web`'s dev server also needs the
 * secret at build time, and it's convenient for both sides to source from the
 * same place. The README documents exporting from Keychain into the shell env.
 */
export function getSharedSecret(): string {
  if (cachedSharedSecret !== null) { return cachedSharedSecret; }

  const fromEnv = process.env.COPILOT_RUNTIME_SECRET?.trim();
  if (fromEnv && fromEnv.length > 0) {
    cachedSharedSecret = fromEnv;
    return cachedSharedSecret;
  }

  cachedSharedSecret = readKeychainSecret(DEFAULT_ACCOUNT, SHARED_SECRET_SERVICE);
  return cachedSharedSecret;
}

/**
 * Test-only: resets the cached secrets so tests can mock different values
 * between cases. Exported from the same module because the cache is module-scoped.
 */
export function __resetCredentialCachesForTests(): void {
  cachedOpenRouterKey = null;
  cachedSharedSecret = null;
}
