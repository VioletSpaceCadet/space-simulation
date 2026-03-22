---
title: "Balance analysis workflow: sim_bench scenarios + MCP advisor"
category: logic-errors
date: 2026-02-24
module: sim_bench, sim_daemon, mcp_advisor
component: runner.rs, analytics, overrides.rs
tags: [balance, sim-bench, scenarios, mcp-advisor, tuning, metrics]
project: Balance & Tuning
tickets: [VIO-6, VIO-8, VIO-9, VIO-11, VIO-12, VIO-14, VIO-15, VIO-16]
---

## Problem

Multiple balance issues were discovered when running the simulation for extended periods:

- **Research stagnation** (VIO-14): Only 1 tech unlocked in 90 days of sim time
- **Repair kit waste** (VIO-11): Maintenance bay consumed kits on trivial wear (0.001)
- **Repair kit oversupply** (VIO-15): After fixing waste, 694 kits stockpiled at 90 days
- **Slag accumulation** (VIO-16): Station storage 45% full of slag at 90 days with no disposal
- **Sustainability gap** (VIO-13): 12-day window with zero maintenance capability

These issues were invisible in the original 2-week (20,160 tick) scenario — they only appeared at longer time horizons.

## Root Cause

The initial balance constants were placeholder values that hadn't been validated against extended gameplay. The original benchmark scenario was too short (2 weeks) to surface accumulation problems and sustainability gaps.

Additionally, there was no way to test module-level parameter changes (processing intervals, wear rates) without editing content files directly.

## Solution

### 1. Extended scenarios (VIO-12)

Added 30-day and 90-day benchmark scenarios to surface long-term balance issues. Key insight: short scenarios hide accumulation problems.

### 2. Module-level overrides in sim_bench (VIO-8)

Extended the override system to support dotted keys for module parameters:

```json
{
  "overrides": {
    "module.basic_iron_refinery.processing_interval_ticks": 120,
    "module.maintenance_bay.processing_interval_ticks": 60
  }
}
```

This enables testing balance changes without editing content files, and supports A/B comparison across parameter sets.

### 3. Constants rebalancing (VIO-9)

Applied balance_v1 changes based on sim evidence:
- Asteroid masses: 100→500,000 min, 100,000→10,000,000 max (realistic small asteroid sizes)
- Mining rates adjusted for 1-tick-per-minute timescale
- Research parameters tuned for ~30-day first-tech-unlock

### 4. Targeted fixes

- **Maintenance threshold** (VIO-11): Added `repair_threshold: 0.1` — maintenance bay only fires when worst wear exceeds 10%, preventing kit waste on trivial wear
- **Slag jettison** (VIO-16): Added jettison command to dispose of slag
- **Starting materials** (VIO-13): Increased starting Fe to sustain early-game assembler

### 5. MCP Balance Advisor

The `mcp_advisor` MCP server connects to the running `sim_daemon` and provides:
- `get_metrics_digest`: trend analysis, rates, bottleneck detection
- `suggest_parameter_change`: data-driven rebalancing proposals
- `query_knowledge`: search past journals and the strategy playbook for relevant context
- `save_run_journal`: persist analysis session findings (observations, bottlenecks, strategy notes)
- `update_playbook`: append to or replace strategy playbook sections

Workflow: recall past knowledge → run sim → analyze metrics via MCP → propose parameter changes → test via sim_bench override → verify with extended scenario → save journal → update playbook if pattern confirmed.

## Prevention

- Always run 90-day scenarios when changing balance constants or module parameters.
- Use sim_bench module overrides to test changes before committing to content files.
- Watch for accumulation metrics: storage utilization, kit counts, tech unlock counts.
- The MCP advisor's `get_metrics_digest` surfaces bottlenecks automatically — use it after any balance change.
- Before starting analysis, call `query_knowledge` to check if the issue has been observed before.
- After completing analysis, call `save_run_journal` to persist findings for future sessions.
- Consult `content/knowledge/playbook.md` for known strategy patterns and parameter relationships.
