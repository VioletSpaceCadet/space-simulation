---
name: Balance Analysis & Knowledge System
triggers: [balance, tuning, playbook, journal, knowledge, bottleneck, parameter change, metrics digest, alert analysis]
agents: [sim-e2e-tester]
---

## When to Use
Balance analysis, tuning investigations, or any work involving the knowledge system (journals, playbook, MCP advisor tools).

## Checklist
- [ ] **Recall first:** Call `query_knowledge` with relevant tags/keywords before starting analysis — check what's already known
- [ ] **Start sim:** Use `start_simulation` (specific seed for reproducibility), then `set_speed(1000)` for fast accumulation
- [ ] **Wait for data:** Need 3000+ ticks (50+ metric samples) before `get_metrics_digest` trends are meaningful
- [ ] **Pause to analyze:** Use `pause_simulation` → read digest/alerts → `resume_simulation` for stable snapshots
- [ ] **Propose changes:** Use `suggest_parameter_change` with rationale — proposals go to `content/advisor_proposals/`
- [ ] **Record findings:** Call `save_run_journal` with observations, bottlenecks, alerts, and strategy notes
- [ ] **Generalize:** When a pattern is confirmed across multiple runs, call `update_playbook` to add it
- [ ] **Clean up:** Always `stop_simulation` when done

## Knowledge Tools (no daemon needed)

| Tool | Purpose |
|------|---------|
| `query_knowledge` | Search journals + playbook by text, tags, or source |
| `save_run_journal` | Persist session findings (auto-generates UUID + timestamp) |
| `update_playbook` | Append to or replace sections of `content/knowledge/playbook.md` |

## Tagging Conventions
Use consistent tags in journals for searchability:
- **System area:** `ore`, `smelting`, `research`, `fleet`, `economy`, `thermal`, `wear`, `propellant`
- **Issue type:** `bottleneck`, `starvation`, `backpressure`, `overshoot`, `stall`
- **Action:** `parameter-change`, `regression-test`, `baseline`

## Testing
- **Single seed:** `start_simulation` → `set_speed(1000)` → analyze via MCP tools
- **Multi-seed:** `cargo run -p sim_bench -- run --scenario scenarios/baseline.json`
- **Smoke:** `./scripts/ci_bench_smoke.sh`

## Pitfalls
- Ship transit takes 2,880 ticks per hop — rates at 0.0 during transit are normal, not a bug
- Short vs long trend windows need 50+ samples to diverge meaningfully
- Alert storms usually cascade from one root cause (e.g. inventory full → starvation → wear)
- Playbook section paths are case-insensitive and use `>` for nesting (e.g. `Bottleneck Resolutions > Ore Supply`)
- `save_run_journal` requires all core fields even if empty arrays — `observations`, `bottlenecks`, `alerts_seen`, `parameter_changes`, `strategy_notes`, `tags`
