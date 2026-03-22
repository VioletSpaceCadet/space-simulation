# Run Journal Schema

A run journal captures observations, bottlenecks, parameter changes, and learnings from a single simulation analysis session. Journals are stored as JSON files in `content/knowledge/journals/`, one file per session.

## Schema

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `string` (UUID v4) | yes | Unique identifier for this journal entry |
| `timestamp` | `string` (ISO-8601) | yes | When the analysis session occurred |
| `seed` | `integer` | yes | Simulation RNG seed used |
| `tick_range` | `[integer, integer]` | yes | `[start_tick, end_tick]` observed during the session |
| `ticks_per_sec` | `number` | no | Simulation speed used during observation |
| `observations` | `Observation[]` | yes | Structured observations from the session (may be empty) |
| `bottlenecks` | `Bottleneck[]` | yes | Bottlenecks detected during the session (may be empty) |
| `alerts_seen` | `JournalAlert[]` | yes | Alerts that fired during the session (may be empty) |
| `parameter_changes` | `ParameterChange[]` | yes | Parameter changes proposed or applied (may be empty) |
| `strategy_notes` | `string[]` | yes | Free-form learnings and insights (may be empty) |
| `tags` | `string[]` | yes | Categorization tags (e.g. `"ore-supply"`, `"fleet-sizing"`, `"slag-management"`) |
| `final_score` | `number` | no | Composite economy/research/fleet metric at run end |
| `collapse_tick` | `integer \| null` | no | Tick when collapse was detected, or `null` if no collapse |
| `bottleneck_timeline` | `BottleneckEvent[]` | no | Time-series of bottleneck state changes (may be empty) |
| `autopilot_config_hash` | `string` | no | Hash of autopilot parameters used (for cross-run comparison) |
| `parquet_path` | `string` | no | Relative path to associated Parquet metrics file |

### Observation

| Field | Type | Required | Description |
|---|---|---|---|
| `metric` | `string` | yes | Metric name (matches MetricsSnapshot field names, e.g. `"total_ore_kg"`, `"station_storage_used_pct"`) |
| `value` | `number` | yes | Observed value at time of note |
| `trend` | `"rising" \| "falling" \| "stable" \| "volatile"` | yes | Direction of change |
| `interpretation` | `string` | yes | What this observation means for the simulation |

### Bottleneck

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `string` | yes | Bottleneck category (e.g. `"ore_starvation"`, `"storage_saturation"`, `"power_deficit"`, `"fleet_idle"`) |
| `severity` | `"low" \| "medium" \| "high" \| "critical"` | yes | Impact severity |
| `tick_range` | `[integer, integer]` | yes | `[start_tick, end_tick]` when bottleneck was active |
| `description` | `string` | yes | Human-readable description of the bottleneck |

### JournalAlert

| Field | Type | Required | Description |
|---|---|---|---|
| `alert_id` | `string` | yes | Alert rule ID (matches AlertEngine IDs, e.g. `"ORE_STARVATION"`, `"STORAGE_SATURATION"`) |
| `severity` | `string` | yes | Alert severity level |
| `first_seen_tick` | `integer` | yes | Tick when alert first fired |
| `resolved_tick` | `integer \| null` | no | Tick when alert cleared, or `null` if still active |

### ParameterChange

| Field | Type | Required | Description |
|---|---|---|---|
| `parameter_path` | `string` | yes | Dotted path to the parameter (e.g. `"constants.survey_scan_ticks"`) |
| `current_value` | `string` | yes | Value before change |
| `proposed_value` | `string` | yes | Value after change |
| `rationale` | `string` | yes | Why the change was made |

### BottleneckEvent

| Field | Type | Required | Description |
|---|---|---|---|
| `tick` | `integer` | yes | Tick when the bottleneck state changed |
| `type` | `string` | yes | Bottleneck category (e.g. `"ore_starvation"`, `"storage_saturation"`) |
| `severity` | `"low" \| "medium" \| "high" \| "critical"` | yes | Severity at this tick |

## File Naming

Journal files are named `{timestamp}_{seed}.json`, e.g. `2026-03-21T14-30-00Z_42.json`. The timestamp uses dashes instead of colons for filesystem compatibility.

## Example

See `content/knowledge/journals/example.json` for a complete example.
