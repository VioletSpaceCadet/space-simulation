"""Outcome labeling functions for sim_bench metrics.

Label functions take a DuckDB relation (from load_run) and return
per-seed outcome labels suitable for ML training.

Consecutive-run detection uses the gaps-and-islands technique:
1. Number all rows with ROW_NUMBER (preserving original ordering)
2. Filter to rows matching the condition
3. Compute island groups: original_rn - ROW_NUMBER_in_filtered
4. Group by island and check tick span against threshold
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import duckdb


def collapse_detection(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Detect economy collapse per seed.

    Collapse conditions (whichever occurs first):
    - balance_zero: balance drops to 0 or below
    - fleet_idle: all ships idle for 500+ consecutive ticks

    Returns:
        Relation with columns: seed, collapse_tick (NULL if no collapse),
        collapse_type ('balance_zero' | 'fleet_idle' | NULL).
    """
    return rel.query(
        "metrics",
        """
        WITH all_seeds AS (
            SELECT DISTINCT seed FROM metrics
        ),
        balance_collapse AS (
            SELECT seed, MIN(tick) AS collapse_tick
            FROM metrics
            WHERE balance <= 0
            GROUP BY seed
        ),
        numbered AS (
            SELECT seed, tick, fleet_idle, fleet_total,
                ROW_NUMBER() OVER (PARTITION BY seed ORDER BY tick) AS rn
            FROM metrics
        ),
        idle_flagged AS (
            SELECT seed, tick, rn
            FROM numbered
            WHERE fleet_idle = fleet_total AND fleet_total > 0
        ),
        idle_islands AS (
            SELECT seed, tick,
                rn - ROW_NUMBER() OVER (PARTITION BY seed ORDER BY rn) AS grp
            FROM idle_flagged
        ),
        idle_runs AS (
            SELECT seed, MIN(tick) AS start_tick
            FROM idle_islands
            GROUP BY seed, grp
            HAVING MAX(tick) - MIN(tick) >= 499
        ),
        fleet_collapse AS (
            SELECT seed, MIN(start_tick) AS collapse_tick
            FROM idle_runs
            GROUP BY seed
        )
        SELECT
            s.seed,
            CASE
                WHEN b.collapse_tick IS NOT NULL AND f.collapse_tick IS NOT NULL
                    THEN CASE WHEN b.collapse_tick <= f.collapse_tick
                        THEN b.collapse_tick ELSE f.collapse_tick END
                ELSE COALESCE(b.collapse_tick, f.collapse_tick)
            END AS collapse_tick,
            CASE
                WHEN b.collapse_tick IS NOT NULL
                    AND (f.collapse_tick IS NULL OR b.collapse_tick <= f.collapse_tick)
                    THEN 'balance_zero'
                WHEN f.collapse_tick IS NOT NULL
                    THEN 'fleet_idle'
                ELSE NULL
            END AS collapse_type
        FROM all_seeds s
        LEFT JOIN balance_collapse b ON s.seed = b.seed
        LEFT JOIN fleet_collapse f ON s.seed = f.seed
        """,
    )


def storage_saturation_tick(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Find the first tick where storage is saturated for 100+ consecutive ticks.

    Saturation: station_storage_used_pct > 0.95 sustained for 100+ ticks.

    Returns:
        Relation with columns: seed, saturation_tick (NULL if no sustained saturation).
    """
    return rel.query(
        "metrics",
        """
        WITH all_seeds AS (
            SELECT DISTINCT seed FROM metrics
        ),
        numbered AS (
            SELECT seed, tick, station_storage_used_pct,
                ROW_NUMBER() OVER (PARTITION BY seed ORDER BY tick) AS rn
            FROM metrics
        ),
        saturated AS (
            SELECT seed, tick, rn
            FROM numbered
            WHERE station_storage_used_pct > 0.95
        ),
        islands AS (
            SELECT seed, tick,
                rn - ROW_NUMBER() OVER (PARTITION BY seed ORDER BY rn) AS grp
            FROM saturated
        ),
        sustained AS (
            SELECT seed, MIN(tick) AS start_tick
            FROM islands
            GROUP BY seed, grp
            HAVING MAX(tick) - MIN(tick) >= 99
        ),
        first_saturation AS (
            SELECT seed, MIN(start_tick) AS saturation_tick
            FROM sustained
            GROUP BY seed
        )
        SELECT s.seed, f.saturation_tick
        FROM all_seeds s
        LEFT JOIN first_saturation f ON s.seed = f.seed
        """,
    )


def research_stall_tick(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Find the first tick where research stalls for 500+ consecutive ticks.

    Stall: total_scan_data stops increasing (delta <= 0) for 500+ ticks,
    only considered after tick 1000 (early game scan ramp-up excluded).

    Returns:
        Relation with columns: seed, stall_tick (NULL if no sustained stall).
    """
    return rel.query(
        "metrics",
        """
        WITH all_seeds AS (
            SELECT DISTINCT seed FROM metrics
        ),
        with_delta AS (
            SELECT seed, tick,
                total_scan_data - LAG(total_scan_data, 1, total_scan_data)
                    OVER (PARTITION BY seed ORDER BY tick) AS scan_delta,
                ROW_NUMBER() OVER (PARTITION BY seed ORDER BY tick) AS rn
            FROM metrics
        ),
        stalled AS (
            SELECT seed, tick, rn
            FROM with_delta
            WHERE scan_delta <= 0 AND tick > 1000
        ),
        islands AS (
            SELECT seed, tick,
                rn - ROW_NUMBER() OVER (PARTITION BY seed ORDER BY rn) AS grp
            FROM stalled
        ),
        sustained AS (
            SELECT seed, MIN(tick) AS start_tick
            FROM islands
            GROUP BY seed, grp
            HAVING MAX(tick) - MIN(tick) >= 499
        ),
        first_stall AS (
            SELECT seed, MIN(start_tick) AS stall_tick
            FROM sustained
            GROUP BY seed
        )
        SELECT s.seed, f.stall_tick
        FROM all_seeds s
        LEFT JOIN first_stall f ON s.seed = f.seed
        """,
    )


def final_score(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Compute composite economy score at the final tick per seed.

    Score is a weighted sum of normalized components:
    - balance (30%): log-scale relative to initial $1B
    - techs_unlocked (30%): fraction of 20 max
    - fleet_total (20%): fraction of 10 max
    - material_throughput (20%): avg material_kg per tick, capped at 1.0

    Returns:
        Relation with columns: seed, score (float), final_tick.
    """
    return rel.query(
        "metrics",
        """
        WITH final_ticks AS (
            SELECT seed, MAX(tick) AS max_tick
            FROM metrics
            GROUP BY seed
        ),
        final_rows AS (
            SELECT m.seed, m.tick, m.balance, m.techs_unlocked,
                m.fleet_total, m.total_material_kg
            FROM metrics m
            JOIN final_ticks f ON m.seed = f.seed AND m.tick = f.max_tick
        )
        SELECT
            seed,
            (0.3 * CASE WHEN balance > 0
                THEN LN(CAST(balance AS DOUBLE) + 1) / LN(1e9 + 1) ELSE 0.0 END
            + 0.3 * LEAST(CAST(techs_unlocked AS DOUBLE) / 20.0, 1.0)
            + 0.2 * LEAST(CAST(fleet_total AS DOUBLE) / 10.0, 1.0)
            + 0.2 * CASE WHEN tick > 0
                THEN LEAST(CAST(total_material_kg AS DOUBLE) / CAST(tick AS DOUBLE), 1.0)
                ELSE 0.0 END
            ) AS score,
            tick AS final_tick
        FROM final_rows
        """,
    )


def bottleneck_timeline(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    """Classify each tick into a bottleneck state and merge into spans.

    Classification priority (first match wins):
    1. StorageFull: station_storage_used_pct > 0.95
    2. WearCritical: max_module_wear > 0.8
    3. SlagBackpressure: slag > 100 AND slag/(material+1) > 0.5
    4. OreSupply: refinery_starved > refinery_active
    5. FleetIdle: fleet_idle > fleet_total/2
    6. Healthy: default

    Returns:
        Relation with columns: seed, tick_start, tick_end, bottleneck_type.
        Spans are non-overlapping and cover the full tick range per seed.
    """
    return rel.query(
        "metrics",
        """
        WITH classified AS (
            SELECT seed, tick,
                CASE
                    WHEN station_storage_used_pct > 0.95 THEN 'StorageFull'
                    WHEN max_module_wear > 0.8 THEN 'WearCritical'
                    WHEN total_slag_kg > 100
                        AND total_slag_kg / (total_material_kg + 1) > 0.5
                        THEN 'SlagBackpressure'
                    WHEN processor_starved > processor_active
                        THEN 'OreSupply'
                    WHEN fleet_total > 0 AND fleet_idle * 2 > fleet_total
                        THEN 'FleetIdle'
                    ELSE 'Healthy'
                END AS bottleneck_type
            FROM metrics
        ),
        with_grp AS (
            SELECT seed, tick, bottleneck_type,
                ROW_NUMBER() OVER (PARTITION BY seed ORDER BY tick)
                - ROW_NUMBER() OVER (PARTITION BY seed, bottleneck_type ORDER BY tick)
                    AS grp
            FROM classified
        )
        SELECT seed,
            MIN(tick) AS tick_start,
            MAX(tick) AS tick_end,
            bottleneck_type
        FROM with_grp
        GROUP BY seed, bottleneck_type, grp
        ORDER BY seed, tick_start
        """,
    )
