/** Structured observation from a simulation analysis session. */
export interface Observation {
  /** Metric name matching MetricsSnapshot fields (e.g. "total_ore_kg"). */
  metric: string;
  /** Observed value at time of note. */
  value: number;
  /** Direction of change. */
  trend: "rising" | "falling" | "stable" | "volatile";
  /** What this observation means for the simulation. */
  interpretation: string;
}

/** A bottleneck detected during the session. */
export interface Bottleneck {
  /** Bottleneck category (e.g. "ore_starvation", "storage_saturation"). */
  type: string;
  /** Impact severity. */
  severity: "low" | "medium" | "high" | "critical";
  /** [start_tick, end_tick] when bottleneck was active. */
  tick_range: [number, number];
  /** Human-readable description. */
  description: string;
}

/** An alert that fired during the session. */
export interface JournalAlert {
  /** Alert rule ID matching AlertEngine IDs (e.g. "ORE_STARVATION"). */
  alert_id: string;
  /** Alert severity level. */
  severity: string;
  /** Tick when alert first fired. */
  first_seen_tick: number;
  /** Tick when alert cleared, or null if still active. */
  resolved_tick: number | null;
}

/** A parameter change proposed or applied during the session. */
export interface ParameterChange {
  /** Dotted path to the parameter (e.g. "constants.survey_scan_ticks"). */
  parameter_path: string;
  /** Value before change. */
  current_value: string;
  /** Value after change. */
  proposed_value: string;
  /** Why the change was made. */
  rationale: string;
}

/**
 * Run journal entry capturing observations, bottlenecks, parameter changes,
 * and learnings from a single simulation analysis session.
 *
 * Stored in content/knowledge/journals/, one file per session.
 * See docs/run-journal-schema.md for the full schema specification.
 */
export interface RunJournal {
  /** Unique identifier (UUID v4). */
  id: string;
  /** When the analysis session occurred (ISO-8601). */
  timestamp: string;
  /** Simulation RNG seed used. */
  seed: number;
  /** [start_tick, end_tick] observed during the session. */
  tick_range: [number, number];
  /** Simulation speed used during observation. */
  ticks_per_sec?: number;
  /** Structured observations from the session. */
  observations: Observation[];
  /** Bottlenecks detected during the session. */
  bottlenecks: Bottleneck[];
  /** Alerts that fired during the session. */
  alerts_seen: JournalAlert[];
  /** Parameter changes proposed or applied. */
  parameter_changes: ParameterChange[];
  /** Free-form learnings and insights. */
  strategy_notes: string[];
  /** Categorization tags (e.g. "ore-supply", "fleet-sizing"). */
  tags: string[];
}
