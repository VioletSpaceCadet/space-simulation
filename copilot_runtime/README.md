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

## Smoke test (Mb1 acceptance)

1. Start `sim_daemon` (`cargo run -p sim_daemon -- run --seed 42`).
2. Start `copilot_runtime` (`npm run dev` from this directory).
3. Start `ui_web` (`cd ../ui_web && npm run dev`) in a shell that has
   `VITE_COPILOT_RUNTIME_SECRET` exported.
4. Open `http://localhost:5173`, click the CopilotKit sidebar, and ask
   something like "what tick are we on?". The LLM should respond via the
   runtime. The Mb1 stub readable returns a fake tick — Mb2 wires the real
   hierarchical snapshot.

You can also hit the health endpoint directly:

```bash
curl http://127.0.0.1:4000/healthz
# → {"status":"ok","provider":"openrouter","model":"qwen/qwen-2.5-72b-instruct"}
```

## Layout

```
copilot_runtime/
├── package.json         # pinned @copilotkit/*, express, ai-sdk
├── tsconfig.json
├── vitest.config.ts
├── src/
│   ├── index.ts         # Express boot, 127.0.0.1:4000
│   ├── runtime.ts       # CopilotRuntime + BuiltInAgent wiring
│   ├── adapter.ts       # env-driven OpenRouter | Ollama factory
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

## Version pinning

CopilotKit is fast-moving and MCP support is new (shipped January 2026).
`package.json` uses exact versions — no `^`, no `~`. Upgrades are intentional,
not automatic. See plan Risk Analysis for the MCP-specific adapter strategy
in Mb4.
