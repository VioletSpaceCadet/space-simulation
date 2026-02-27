export const DAEMON_URL = "http://localhost:3002";

/** POST to a daemon endpoint, throwing on non-2xx responses. */
export async function daemonPost(
  path: string,
  body?: object,
): Promise<void> {
  const res = await fetch(`${DAEMON_URL}${path}`, {
    method: "POST",
    ...(body
      ? {
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        }
      : {}),
  });
  if (!res.ok) {
    throw new Error(`Daemon POST ${path} failed: ${res.status}`);
  }
}

/** GET a daemon endpoint and return parsed JSON, throwing on non-2xx. */
export async function daemonGet<T = Record<string, unknown>>(
  path: string,
): Promise<T> {
  const res = await fetch(`${DAEMON_URL}${path}`);
  if (!res.ok) {
    throw new Error(`Daemon GET ${path} failed: ${res.status}`);
  }
  return res.json() as Promise<T>;
}

interface MetaResponse {
  tick: number;
  ticks_per_sec: number;
  paused: boolean;
  seed: number;
  trade_unlock_tick: number;
  content_version: string;
}

/** Fetch daemon /meta with typed response. */
export async function getMeta(): Promise<MetaResponse> {
  return daemonGet<MetaResponse>("/api/v1/meta");
}
