---
title: Python DuckDB ML Pipeline — Patterns and Gotchas
category: integration-issues
date: 2026-03-23
tags: [python, duckdb, mypy, ruff, coverage, sql, gaps-and-islands, ml-pipeline]
components: [scripts/analysis]
---

## Problem

Building a Python ML data pipeline (DuckDB + pyarrow) within an existing Rust + TypeScript monorepo required solving multiple integration challenges: type safety with mypy strict, DuckDB relation API patterns, consecutive-run detection in SQL, and coverage threshold management.

## Key Patterns

### 1. DuckDB `rel.query()` for Complex SQL

Simple column expressions use `rel.select(...)`. For complex SQL requiring CTEs, window functions, or subqueries, use `rel.query(virtual_table_name, sql)`:

```python
def my_analysis(rel: duckdb.DuckDBPyRelation) -> duckdb.DuckDBPyRelation:
    return rel.query("metrics", """
        WITH cte AS (
            SELECT seed, tick, ROW_NUMBER() OVER (...) AS rn
            FROM metrics
        )
        SELECT ... FROM cte
    """)
```

The relation is accessible as the virtual table name ("metrics") throughout the SQL, including CTEs.

### 2. Gaps-and-Islands for Consecutive Run Detection

To find consecutive ticks where a condition holds (e.g., "storage > 95% for 100+ ticks"):

```sql
-- Step 1: Number ALL rows (including non-qualifying)
numbered AS (
    SELECT seed, tick, condition_column,
        ROW_NUMBER() OVER (PARTITION BY seed ORDER BY tick) AS rn
    FROM metrics
),
-- Step 2: Filter to qualifying rows (preserving original rn)
flagged AS (
    SELECT seed, tick, rn FROM numbered
    WHERE condition_column > threshold
),
-- Step 3: Island detection — consecutive rn values get same group
islands AS (
    SELECT seed, tick,
        rn - ROW_NUMBER() OVER (PARTITION BY seed ORDER BY rn) AS grp
    FROM flagged
),
-- Step 4: Check tick span per island
qualifying AS (
    SELECT seed, MIN(tick) AS start_tick
    FROM islands GROUP BY seed, grp
    HAVING MAX(tick) - MIN(tick) >= 499  -- 500+ ticks
)
```

**Critical:** The ROW_NUMBER in step 1 must be computed on ALL rows before filtering. If computed only on filtered rows, rn is sequential and the island detection collapses to a single group.

**Tick threshold:** Use `MAX(tick) - MIN(tick) >= N-1` for "N+ consecutive ticks". This works for any `metrics_every` value because tick values are the actual game ticks, not sample indices.

### 3. mypy Strict with `dict[str, object]`

mypy strict rejects `int(dict_value)` when the dict type is `dict[str, object]` (error: `No overload variant of "int" matches argument type "object"`).

Fix: use `isinstance` assertion before arithmetic:
```python
for s in all_stats.values():
    seeds = s["seeds"]
    assert isinstance(seeds, int)
    total += seeds
```

### 4. Coverage Management for CLI Scripts

Scripts with `main()` functions that require cargo builds or external data can't be unit tested. Use `# pragma: no cover` on functions that need runtime dependencies:

```python
def run_sim_bench(path: Path, out: Path) -> None:  # pragma: no cover
    subprocess.run(["cargo", "run", ...], check=True)
```

The `pyproject.toml` already excludes `if __name__ == "__main__":` and `if TYPE_CHECKING:` blocks from coverage. The 75% threshold was set specifically to accommodate untested CLI entry points.

### 5. ruff Import Sorting with `from __future__ import annotations`

ruff I001 flags import blocks as unsorted when there's a blank line between `from __future__ import annotations` and `import duckdb`. Run `ruff check --fix` to auto-sort, or ensure the import block has no extra blank lines within the same section.

### 6. DuckDB `STDDEV_SAMP` Returns NULL for n=1

When computing cross-seed statistics with a single seed, `STDDEV_SAMP` returns NULL (divides by n-1=0). Always wrap with `COALESCE(..., 0.0)`:

```sql
COALESCE(STDDEV_SAMP(value), 0.0) AS stddev
```

### 7. SQL String Interpolation and ruff S608

Ruff S608 flags f-string SQL as potential injection. For module-internal functions where values come from other DuckDB queries (not user input), add `# noqa: S608`. For constants, hardcode them directly in the SQL string instead.

## Prevention

- Always compute ROW_NUMBER on the full dataset before filtering for gaps-and-islands
- Use `COALESCE` around all aggregate functions that can return NULL (STDDEV_SAMP, VAR_SAMP)
- Mark CLI orchestration functions with `# pragma: no cover` to keep coverage above threshold
- Run `ruff check --fix` after editing imports — it auto-sorts correctly
- Use `isinstance` assertions, not casts, for mypy strict with generic dict values
