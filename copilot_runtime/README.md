# copilot_runtime

CopilotKit runtime sidecar for the space simulation LLM co-pilot. Local
Express server that routes chat requests from `ui_web` to an OpenAI-compatible
LLM provider (OpenRouter now, Ollama later).

This crate exists because:

- CopilotKit's runtime must run in a Node process. Embedding it in Vite
  middleware is fragile — the canonical 2026 pattern is a standalone sibling
  to the frontend dev server.
- API keys live in the macOS Keychain, not `.env` files (plan Decision 13), so
  the runtime needs to read secrets at startup from a process that can call
  `security(1)`.
- Shared-secret localhost hardening belongs here, before the CopilotKit
  handler dispatches to the LLM.

See the **CopilotKit Foundation** project in Linear (VioletSpaceCadet
workspace) and `docs/plans/2026-04-11-001-feat-sim-optimization-variance-plan.md`
§ Project B for the full architecture. The plan landed in a separate commit
— if the file is missing at your checkout, the Linear project description
carries the same decisions.

## Local dev stack

```
sim_daemon       → :3001   (game loop, HTTP + SSE)
ui_web (vite)    → :5173   (React mission control)
copilot_runtime  → :4000   (this process)
ollama (Phase B) → :11434  (local inference)
```

All four bind to loopback. No off-host traffic.

## First-time setup

### 1. Install dependencies

```bash
cd copilot_runtime
npm install
```

### 2. Store the OpenRouter API key in macOS Keychain

The OpenRouter key never lives in a dotfile or environment variable. Add it
once via `security(1)`:

```bash
security add-generic-password \
  -a "copilot_runtime" \
  -s "OPENROUTER_API_KEY" \
  -w "sk-or-..."
```

If you skip this, `copilot_runtime` refuses to start and prints the exact
command above.

### 3. Create a localhost shared secret

`ui_web` and `copilot_runtime` share a pre-negotiated secret so other local
processes on your machine cannot hit the runtime endpoint. Generate one and
store it in Keychain:

```bash
security add-generic-password \
  -a "copilot_runtime" \
  -s "COPILOT_RUNTIME_SECRET" \
  -w "$(openssl rand -hex 32)"
```

At dev time, export it into your shell so Vite picks it up:

```bash
# Add to ~/.zshrc or source before `npm run dev`:
export COPILOT_RUNTIME_SECRET="$(security find-generic-password -a copilot_runtime -s COPILOT_RUNTIME_SECRET -w)"
export VITE_COPILOT_RUNTIME_SECRET="$COPILOT_RUNTIME_SECRET"
```

`copilot_runtime` itself reads the secret directly from Keychain if
`COPILOT_RUNTIME_SECRET` is unset, so this export is optional for the server
side but required for `ui_web` (Vite cannot call `security(1)`).

## Running

```bash
# Start the sidecar. Defaults: provider=openrouter, port=4000.
npm run dev            # tsx watch, live reload
npm start              # built output from dist/
npm run build          # tsc → dist/
```

### Environment variables

| Var                      | Default                          | Meaning |
|--------------------------|----------------------------------|---------|
| `LLM_PROVIDER`           | `openrouter`                     | `openrouter` or `ollama` |
| `LLM_MODEL`              | provider-specific default        | Override the default model name |
| `COPILOT_RUNTIME_PORT`   | `4000`                           | Loopback port to bind |
| `COPILOT_UI_ORIGIN`      | `http://localhost:5173`          | CORS origin allowlist |
| `COPILOT_RUNTIME_SECRET` | read from Keychain if unset      | Shared secret for localhost hardening |

Defaults per provider:

| Provider     | Default model                  | Base URL                        |
|--------------|--------------------------------|---------------------------------|
| `openrouter` | `qwen/qwen-2.5-72b-instruct`   | `https://openrouter.ai/api/v1`  |
| `ollama`     | `qwen2.5:14b-instruct`         | `http://localhost:11434/v1`     |

## MCP integration (balance-advisor)

At startup, `copilot_runtime` spawns `mcp_advisor` as a stdio child process
and connects via `@ai-sdk/mcp`. The LLM gains access to 5 read-only
analytics tools:

| Tool | What it provides |
|------|-----------------|
| `get_metrics_digest` | Trends, production rates, bottleneck analysis |
| `get_active_alerts` | Current alert list from the daemon |
| `get_game_parameters` | Game content files (constants, techs, pricing) |
| `query_knowledge` | Past run journals and strategy playbook |
| `get_strategy_config` | Current autopilot strategy settings |

Write tools (sim lifecycle, parameter proposals, knowledge mutations) are
filtered out — sim control is covered by the approval-card actions, and
admin tools aren't player-facing.

If `mcp_advisor` isn't built (`npm run build` in `mcp_advisor/`), the
sidecar starts without analytics tools and logs a warning. Build with:

```bash
cd ../mcp_advisor && npm run build
```

The MCP adapter (`src/mcp.ts`) is the single file that touches the MCP
client API. If CopilotKit or `@ai-sdk/mcp` change upstream, breakage is
contained here.

## Smoke test

1. Build `mcp_advisor` (`cd ../mcp_advisor && npm run build`).
2. Start `sim_daemon` (`cargo run -p sim_daemon -- run --seed 42`).
3. Start `copilot_runtime` (`npm run dev` from this directory).
   Console should show: `MCP advisor connected (5 read-only tools)`.
4. Start `ui_web` (`cd ../ui_web && npm run dev`).
5. Open `http://localhost:5173`. The CopilotKit sidebar shows
   "Mission Co-pilot · running ⟳".
6. Ask "how many ships do I have?" — should show a FleetTable card.
7. Ask "pause the sim" — should show an approval card with Pause button.
8. Click Pause — sim should pause, card shows "APPROVED".
9. Ask "what are the current bottlenecks?" — should invoke
   `get_metrics_digest` and summarize the analytics.

You can also hit the health endpoint directly:

```bash
curl http://127.0.0.1:4000/healthz
# → {"status":"ok","provider":"openrouter","model":"qwen/qwen-2.5-72b-instruct"}
```

## Layout

```
copilot_runtime/
├── package.json         # pinned @copilotkit/*, express, ai-sdk, @modelcontextprotocol/sdk
├── tsconfig.json
├── vitest.config.ts
├── src/
│   ├── index.ts         # Express boot, 127.0.0.1:4000
│   ├── runtime.ts       # CopilotRuntime + BuiltInAgent wiring
│   ├── adapter.ts       # env-driven OpenRouter | Ollama factory
│   ├── mcp.ts           # MCP client adapter for balance-advisor (decision 4)
│   ├── credentials.ts   # macOS Keychain retrieval (decision 13)
│   ├── auth.ts          # shared-secret middleware
│   └── *.test.ts        # vitest unit tests
└── README.md            # you are here
```

## Testing

```bash
npm test          # vitest run
npm run typecheck # tsc --noEmit
npm run lint      # eslint --max-warnings=0
```

Tests stub `node:child_process` and `createOpenAICompatible` so they never
touch Keychain, the network, or a real LLM. CI runs these; manual smoke
testing (step above) covers the parts the unit tests cannot.

## Known workarounds

### AI SDK `txt-0` stream-part ID collision

`@ai-sdk/openai-compatible` hardcodes `id: "txt-0"` on every text stream
part. CopilotKit's `BuiltInAgent` forwards that ID verbatim as the AG-UI
messageId, and the client's `deduplicateMessages()` merges all assistant
turns into a single bubble.

Fix: `src/languageModelMiddleware.ts` wraps the chat model with a Proxy
that intercepts `doStream()` and rewrites text-part IDs to fresh UUIDs.
If a future CopilotKit or AI SDK release fixes the upstream issue, delete
`languageModelMiddleware.ts` and the `wrapChatModelWithUniqueTextIds` call
in `adapter.ts`.

See `docs/solutions/integration-issues/copilotkit-ai-sdk-stream-id-deduplication.md`
for the full root-cause analysis.

### CopilotKit v1/v2 import mixing

All server-side CopilotKit imports MUST come from `@copilotkit/runtime/v2`
(and `/v2/express`). Mixing v1 `CopilotRuntime` with v2 `BuiltInAgent`
silently serves the wrong wire format, producing `Agent default not found`
errors on the client.

## Switching to Ollama (Phase A → Phase B)

When the Mac Mini M4 Pro arrives (or any machine with enough VRAM for
local inference), switch from OpenRouter to Ollama in two steps:

### 1. Install and pull a model

```bash
# Install Ollama (macOS)
brew install ollama

# Pull the default model (8.4 GB, Q4_K_M quantization)
ollama pull qwen2.5:14b-instruct

# Optional: larger model for better tool calling
ollama pull qwen3:30b-a3b

# Keep models warm to avoid cold-start latency
export OLLAMA_KEEP_ALIVE=30m
```

### 2. Start copilot_runtime with Ollama

```bash
# Start Ollama (if not already running as a service)
ollama serve

# Start copilot_runtime pointed at Ollama
LLM_PROVIDER=ollama npm run dev
```

That's it. The adapter factory in `adapter.ts` handles everything:
- Skips the macOS Keychain call (Ollama ignores API keys)
- Points at `http://localhost:11434/v1` (Ollama's OpenAI-compatible endpoint)
- Uses `qwen2.5:14b-instruct` by default (override with `LLM_MODEL=...`)
- The `txt-0` stream-ID middleware wraps Ollama the same way it wraps OpenRouter

To switch back: unset `LLM_PROVIDER` (or set it to `openrouter`) and restart.

### Model selection guidance

| Model | VRAM | Speed | Tool calling | Notes |
|-------|------|-------|-------------|-------|
| `qwen2.5:14b-instruct` | ~10 GB | Fast | Good | Default, recommended for most use |
| `qwen3:30b-a3b` | ~18 GB | Moderate | Better | MoE, stronger reasoning |
| `qwen2.5:72b-instruct` | ~48 GB | Slow | Best | Only if you have the VRAM |

### Localhost hardening

Ollama binds to `127.0.0.1:11434` by default. Verify with:

```bash
lsof -i :11434 -Pn
# Should show ollama on 127.0.0.1, not *
```

If Ollama is exposed on `0.0.0.0`, set `OLLAMA_HOST=127.0.0.1` before starting.

## Version pinning

CopilotKit is fast-moving and MCP support is new (shipped January 2026).
`package.json` uses exact versions — no `^`, no `~`. Upgrades are intentional,
not automatic. See plan Risk Analysis for the MCP-specific adapter strategy
in Mb4.
