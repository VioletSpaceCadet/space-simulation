import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock node:child_process BEFORE importing the module under test so the module's
// top-level bindings pick up our mock.
vi.mock("node:child_process", () => ({
  execFileSync: vi.fn(),
}));

import { execFileSync } from "node:child_process";
import {
  __resetCredentialCachesForTests,
  getOpenRouterKey,
  getSharedSecret,
  readKeychainSecret,
} from "./credentials.js";

const mockedExecFileSync = vi.mocked(execFileSync);

describe("credentials", () => {
  beforeEach(() => {
    __resetCredentialCachesForTests();
    mockedExecFileSync.mockReset();
    delete process.env.COPILOT_RUNTIME_SECRET;
  });

  afterEach(() => {
    delete process.env.COPILOT_RUNTIME_SECRET;
  });

  describe("readKeychainSecret", () => {
    it("returns the trimmed secret from `security find-generic-password`", () => {
      mockedExecFileSync.mockReturnValueOnce("sk-or-super-secret\n");

      const result = readKeychainSecret("copilot_runtime", "OPENROUTER_API_KEY");

      expect(result).toBe("sk-or-super-secret");
      expect(mockedExecFileSync).toHaveBeenCalledWith(
        "security",
        [
          "find-generic-password",
          "-a",
          "copilot_runtime",
          "-s",
          "OPENROUTER_API_KEY",
          "-w",
        ],
        expect.objectContaining({ encoding: "utf8" }),
      );
    });

    it("throws with install instructions when the keychain entry is missing", () => {
      mockedExecFileSync.mockImplementationOnce(() => {
        throw new Error("security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.");
      });

      expect(() =>
        readKeychainSecret("copilot_runtime", "OPENROUTER_API_KEY"),
      ).toThrowError(
        /missing Keychain entry.*copilot_runtime.*OPENROUTER_API_KEY.*security add-generic-password/s,
      );
    });

    it("passes arguments as a list so shell metacharacters cannot inject", () => {
      mockedExecFileSync.mockReturnValueOnce("value\n");

      readKeychainSecret('malicious"; rm -rf /', "service");

      const [command, args] = mockedExecFileSync.mock.calls[0]!;
      expect(command).toBe("security");
      expect(args).toEqual([
        "find-generic-password",
        "-a",
        'malicious"; rm -rf /',
        "-s",
        "service",
        "-w",
      ]);
    });
  });

  describe("getOpenRouterKey", () => {
    it("reads the key lazily and caches it across calls", () => {
      mockedExecFileSync.mockReturnValueOnce("sk-or-key-1\n");

      expect(getOpenRouterKey()).toBe("sk-or-key-1");
      expect(getOpenRouterKey()).toBe("sk-or-key-1");
      expect(mockedExecFileSync).toHaveBeenCalledTimes(1);
    });

    it("surfaces the install-instruction error when Keychain retrieval fails", () => {
      mockedExecFileSync.mockImplementationOnce(() => {
        throw new Error("not found");
      });

      expect(() => getOpenRouterKey()).toThrowError(/missing Keychain entry/);
    });

    it("refuses to cache an empty Keychain value", () => {
      mockedExecFileSync.mockReturnValueOnce("   \n");

      // Empty entry must throw with install instructions instead of caching
      // an empty string that silently causes 401s on every subsequent call.
      expect(() => getOpenRouterKey()).toThrowError(
        /Keychain entry.*is empty.*security add-generic-password/s,
      );

      // Calling again re-attempts the Keychain read; nothing is cached.
      mockedExecFileSync.mockReturnValueOnce("sk-or-fresh\n");
      expect(getOpenRouterKey()).toBe("sk-or-fresh");
    });
  });

  describe("getSharedSecret", () => {
    it("prefers COPILOT_RUNTIME_SECRET env var when set", () => {
      process.env.COPILOT_RUNTIME_SECRET = "env-provided-secret";

      expect(getSharedSecret()).toBe("env-provided-secret");
      expect(mockedExecFileSync).not.toHaveBeenCalled();
    });

    it("falls back to Keychain when env var is unset", () => {
      mockedExecFileSync.mockReturnValueOnce("keychain-secret\n");

      expect(getSharedSecret()).toBe("keychain-secret");
      expect(mockedExecFileSync).toHaveBeenCalledTimes(1);
    });

    it("trims whitespace from env var values", () => {
      process.env.COPILOT_RUNTIME_SECRET = "  padded-secret  ";

      expect(getSharedSecret()).toBe("padded-secret");
    });

    it("does not treat an empty env var as a valid secret", () => {
      process.env.COPILOT_RUNTIME_SECRET = "   ";
      mockedExecFileSync.mockReturnValueOnce("keychain-secret\n");

      expect(getSharedSecret()).toBe("keychain-secret");
    });

    it("caches the secret across calls", () => {
      mockedExecFileSync.mockReturnValueOnce("keychain-secret\n");

      getSharedSecret();
      getSharedSecret();

      expect(mockedExecFileSync).toHaveBeenCalledTimes(1);
    });

    it("refuses to cache an empty Keychain value", () => {
      mockedExecFileSync.mockReturnValueOnce("\n");

      expect(() => getSharedSecret()).toThrowError(
        /Keychain entry.*is empty.*openssl rand/s,
      );
    });
  });
});
