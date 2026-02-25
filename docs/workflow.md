# Development Workflow

## Quick Reference

```bash
# One-time setup
./scripts/install_hooks.sh          # Install git hooks

# Local checks (same as CI)
./scripts/ci_rust.sh                # fmt + clippy + test
./scripts/ci_web.sh                 # npm ci + lint + tsc + vitest
./scripts/ci_bench_smoke.sh         # Build release + run ci_smoke scenario

# Gate check on bench artifacts
./scripts/ci_check_summary.sh artifacts

# Bypass hooks when needed
SKIP_HOOKS=1 git commit -m "wip"
SKIP_HOOKS=1 git push
```

## What Runs When

| Event | Rust (fmt/clippy/test) | Web (lint/tsc/test) | Bench smoke | Gate check |
|-------|:---:|:---:|:---:|:---:|
| `git commit` (local hook) | fmt + clippy | eslint + tsc | -- | -- |
| `git push` (local hook) | cargo test | vitest | -- | -- |
| PR opened/updated | yes | yes | yes | yes (no collapse) |
| Push to main | yes | yes | yes | yes (no collapse) |

### CI Jobs

**ci.yml** triggers on `push` (main) and `pull_request` (all branches):

1. **rust** — `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
2. **web** — `npm ci`, `npm run lint`, `npx tsc -b --noEmit`, `npm test`
3. **bench-smoke** — builds sim_bench (release), runs `ci_smoke.json` (2000 ticks, 2 seeds), uploads artifacts. Depends on `rust` passing first.

### Caching

- **Rust:** `Swatinem/rust-cache@v2` caches cargo registry + target dir
- **Node:** `actions/setup-node@v4` caches `npm` based on `package-lock.json`

### Concurrency

- CI: grouped by ref, cancels in-progress on new push
- Nightly: single group, cancels in-progress

## Gate Checks (v1)

`ci_check_summary.sh` parses `batch_summary.json` and enforces:

| Gate | Threshold | Rationale |
|------|-----------|-----------|
| No collapses | `collapsed_count == 0` | Sim must not enter collapse state |

To add more gates, edit `scripts/ci_check_summary.sh`. Future candidates:
- `techs_unlocked >= N`
- `avg_module_wear < threshold`
- `fleet_idle_pct < threshold`

## Artifacts

CI uploads bench artifacts to GitHub Actions:

```
artifacts/
  batch_summary.json    # Aggregated metrics (mean/min/max/stddev)
  summary.json          # Run completion summary
  ci_smoke_<timestamp>/ # Full run output
    seed_1/
      run_result.json   # Per-seed result (schema v1)
      metrics_000.csv   # Time-series metrics
    seed_2/
      ...
```

### Interpreting batch_summary.json

Key fields:
- `collapsed_count`: seeds that entered collapse (refinery starved + fleet idle). Should be 0.
- `aggregated_metrics.*`: cross-seed statistics for each metric
- `seed_count`: how many seeds ran

### Interpreting run_result.json

Key fields:
- `run_status`: `completed` (good), `collapsed` (bad), `error` (bug)
- `summary_metrics`: final-tick snapshot of all sim metrics
- `wall_time_ms` / `sim_ticks_per_second`: performance data
- `collapse_tick` / `collapse_reason`: if collapsed, when and why

## Local Git Hooks

### Installation

```bash
./scripts/install_hooks.sh
```

This sets `core.hooksPath=.githooks`. One-time per clone.

### Hooks

| Hook | What it does | Speed |
|------|-------------|-------|
| `pre-commit` | `cargo fmt --check` + `cargo clippy` + `eslint` + `tsc` | ~10–20s |
| `pre-push` | `cargo test` + `vitest` | ~30–60s |

### Bypassing

```bash
SKIP_HOOKS=1 git commit -m "wip: unfinished"
SKIP_HOOKS=1 git push
# or
git commit --no-verify -m "wip"
```

Use sparingly. CI will still catch issues.

## PR Conventions

1. **Small PRs** — one feature or fix per PR
2. **Branch naming** — `feat/<name>`, `fix/<name>`, `docs/<name>`
3. **Squash merge** — always squash when merging to main
4. **Scenario evidence** — for balance/sim changes, include bench results in PR description
5. **Wait for CI** — all jobs must pass before merging

### PR Description Template

```markdown
## Summary
- What changed and why (1–3 bullets)

## Test plan
- [ ] cargo test passes
- [ ] vitest passes
- [ ] bench smoke passes (no collapses)

## Bench results (if applicable)
<paste relevant batch_summary.json metrics>
```

## Scenarios

| Scenario | Ticks | Seeds | Use |
|----------|-------|-------|-----|
| `ci_smoke.json` | 2,000 | 2 | CI smoke test (~2s) |
| `baseline.json` | 20,160 | 5 | Current defaults (2 weeks) |
| `balance_v1.json` | 20,160 | 5 | Module tuning proposals |
| `cargo_sweep.json` | 10,000 | 5 | Cargo capacity stress test |
| `month.json` | 43,200 | 3 | 30-day sustainability |
| `quarter.json` | 129,600 | 3 | 90-day long-term |

## Balance & Tuning Loop

1. **Run sim_bench scenarios** — `scenarios/baseline.json` (current defaults) or custom scenario with overrides
2. **Analyze results** — inspect `batch_summary.json` aggregated metrics, per-seed `run_result.json`, and `metrics_000.csv` time series
3. **File Linear tickets** — create issues with sim data, proposed changes, and rationale
4. **Test via overrides** — use `module.*` dotted keys in scenario overrides to test changes without editing content files
5. **Apply to content** — once validated, update `content/constants.json` or `content/module_defs.json`
6. **Re-run and verify** — confirm metrics improve, no regressions

## Content & Starting State

- `content/dev_base_state.json` — canonical starting state for gameplay testing (refinery, assembler, maintenance bay, 2 labs, 500 kg Fe, 10 repair kits, 50 m³ ship cargo, 2,000 m³ station cargo)
- `content/constants.json` — game constants (already rebalanced for hard sci-fi pacing)
- `content/module_defs.json` — module behavior parameters (intervals, wear, recipes)
- `build_initial_state()` in sim_world should stay in sync with `dev_base_state.json`

Scenarios support: `"state"` (path to initial state JSON), `"overrides"` (constants + `module.*` keys), `"seeds"` (list or `{"range": [1, 5]}`).

---

## Claude Code: GitHub MCP Setup

> **Note:** The `gh` CLI (already configured) is sufficient for all GitHub operations in Claude Code. The MCP server below is optional and provides additional tool-based integration.

### Prerequisites

1. **GitHub CLI** — install if missing:
   ```bash
   brew install gh          # macOS
   ```

2. **Authenticate gh**:
   ```bash
   gh auth login            # Browser flow (recommended)
   gh auth status           # Verify: ✓ Logged in to github.com
   ```

3. **Required scopes**: `repo`, `read:org` (if using org repos)
   ```bash
   gh auth refresh -s repo,read:org
   ```

### Configure MCP Server

Add the GitHub MCP server to Claude Code's config. The config file lives at:
```
~/.claude/mcp_servers.json
```

Add an entry:
```json
{
  "github": {
    "type": "stdio",
    "command": "npx",
    "args": ["-y", "@anthropic-ai/github-mcp-server"],
    "env": {
      "GITHUB_TOKEN": "<your-token>"
    }
  }
}
```

**Getting the token:**
```bash
# Option A: Use gh CLI token directly
gh auth token    # Prints your current token

# Option B: Create a fine-grained PAT at https://github.com/settings/tokens
# Scopes needed: repo (full), read:org
```

Then paste the token into the config, or set it dynamically:
```json
{
  "github": {
    "type": "stdio",
    "command": "npx",
    "args": ["-y", "@anthropic-ai/github-mcp-server"],
    "env": {
      "GITHUB_TOKEN_CMD": "gh auth token"
    }
  }
}
```

### Verify It Works

After configuring, restart Claude Code and test:
```
> /mcp
```
You should see `github` listed as a connected server.

Try a trivial call in Claude Code:
- "List my repos" or "Show open PRs for this repo"

### Common Failures

| Symptom | Cause | Fix |
|---------|-------|-----|
| `401 Unauthorized` | Token expired or missing scopes | `gh auth refresh -s repo,read:org` |
| SSO error | Org requires SSO authorization | `gh auth refresh` then authorize at the SSO prompt, or visit github.com/settings/tokens and authorize SSO for the token |
| Wrong account | gh logged into personal, repo is org | `gh auth login` with correct account |
| `npx` not found | Node.js not in PATH | Ensure `node`/`npx` are available in the shell Claude Code uses |
| MCP server not listed | Config file syntax error | Validate JSON: `cat ~/.claude/mcp_servers.json | jq .` |
| Timeout on connect | Network/firewall | Check `npx -y @anthropic-ai/github-mcp-server --version` works |

### Smoke Test Checklist

- [ ] `gh auth status` shows correct account with `repo` scope
- [ ] `gh api repos/:owner/:repo` returns repo data (not 401/404)
- [ ] Claude Code `/mcp` shows `github` as connected
- [ ] Claude Code can run "list open PRs" without error
