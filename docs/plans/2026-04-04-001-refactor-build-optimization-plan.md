---
title: Build Optimization — Linker, Crate Splitting, Nextest, Test Migration
type: refactor
status: active
date: 2026-04-04
---

# Build Optimization — Linker, Crate Splitting, Nextest, Test Migration

## Overview

Evaluate and plan 4 build optimization tickets (VIO-635 through VIO-638) for the Rust workspace. All measurements taken on the current codebase (54,547 Rust LoC, 6 crates, 912 tests, macOS Apple Silicon).

## Current State (Baseline Measurements)

| Metric | Value |
|--------|-------|
| Clean `cargo check` | 19.0s wall / 70s CPU |
| Clean `cargo test --no-run` | 55.8s wall / 231s CPU |
| Incremental `cargo test` (run all) | 8.3s wall |
| sim_core compile time | 14.3s (82% in LLVM codegen) |
| sim_core LoC | 35,770 (71% of workspace) |
| Binary link steps | 3 (daemon 4.6s, bench 2.6s, cli 1.8s) |
| Test binary link steps | 12 total (6 lib + 6 integration) |
| `.cargo/config.toml` | Does not exist |
| Profile settings | None (all Cargo defaults) |

## Ticket Evaluations

### VIO-635: Use mold/lld Linker — RECOMMENDED FIRST

**Impact: High | Effort: Low | Risk: Low**

**Why it matters:** No `.cargo/config.toml` exists — the project uses the macOS default linker for everything. There are 3 binary link steps (9.0s total) plus 12 test binary link steps. Test compilation (55.8s) is 3x check time (19s), largely due to linking 12 separate test binaries.

**Approach:**
- On macOS: Use `rust-lld` (available since rustc 1.93, project runs 1.93.1)
  - Set via `-Clinker-flavor=gnu-lld` or `-Zlinker-features=+lld` (nightly)
  - Alternative: Install `lld` via Homebrew, configure as linker
- On Linux CI: Use `mold` (faster than lld for ELF targets)
- Create `.cargo/config.toml` with platform-conditional linker settings

**Expected gains:** 30-60% reduction in link times. For 12 test binaries, this could save 5-15s on clean test compilation. The 3 production binary links (9s) could drop to 3-5s.

**Bonus optimizations to include in this ticket:**
- Add `[profile.dev]` with `split-debuginfo = "unpacked"` (faster macOS debug builds)
- Consider `codegen-units = 512` for dev (more parallel LLVM, slightly larger binaries)
- Add `[profile.dev.package."*"]` with `opt-level = 1` for faster-running deps in dev

**Acceptance Criteria:**
- [ ] `.cargo/config.toml` configures faster linker for macOS dev and Linux CI
- [ ] `[profile.dev]` tuned for dev iteration speed
- [ ] Before/after measurements in PR description (clean check, clean test --no-run, incremental test)
- [ ] CI green on all platforms

**Files to create/modify:**
- Create: `.cargo/config.toml`
- Modify: `Cargo.toml` (workspace profile settings)
- Modify: `.github/workflows/ci.yml` (install mold on Linux runners if needed)

---

### VIO-638: Move Integration Tests to Lib Tests — RECOMMENDED SECOND

**Impact: Medium | Effort: Medium | Risk: Low-Medium**

**Why it matters:** Each `tests/*.rs` file produces a **separate test binary** with its own link step. The 6 integration test files add 6 extra link steps to every `cargo test` run. Moving them into `src/tests/` (like sim_core already does) eliminates those link steps entirely.

**Current integration test files (6 files, 3,981 lines):**

| File | Crate | Lines | Notes |
|------|-------|-------|-------|
| `sim_world/tests/content_validation.rs` | sim_world | 1,814 | Largest — validates content JSON |
| `sim_control/tests/progression.rs` | sim_control | 1,435 | Full progression lifecycle |
| `sim_bench/tests/manufacturing_integration.rs` | sim_bench | 297 | Manufacturing scenarios |
| `sim_core/tests/research_lifecycle.rs` | sim_core | 151 | Requires `test-support` feature |
| `sim_control/tests/sim_events_integration.rs` | sim_control | 154 | Event integration checks |
| `sim_bench/tests/thermal_scenario.rs` | sim_bench | 130 | Thermal scenarios |

**Approach per crate:**
1. **sim_core** (1 file, 151 lines): Already has `src/tests/` directory pattern. Move `tests/research_lifecycle.rs` → `src/tests/research_lifecycle.rs`, add `mod research_lifecycle` in tests module. Remove `test-support` feature gate (lib tests already have access).
2. **sim_control** (2 files, 1,589 lines): Create `src/tests/` directory following sim_core's pattern. Move both files. Add `mod tests` in `lib.rs` behind `#[cfg(test)]`.
3. **sim_world** (1 file, 1,814 lines): Same pattern. Create `src/tests/`, move content_validation.rs.
4. **sim_bench** (2 files, 427 lines): Same pattern. Create `src/tests/`, move both files.

**Gotchas from institutional learnings:**
- `serde(default = "fn_name")` functions must stay in the same file as the struct (relevant if tests reference content paths)
- After any file move, grep `scripts/ci_*.sh` for hardcoded paths (learned from VIO-411)
- Use `crate::` imports, not `use super::*` from submodules

**Acceptance Criteria:**
- [ ] All 6 integration test files moved to `src/tests/` directories
- [ ] Zero `tests/` integration directories remain in workspace
- [ ] Test count unchanged (912 tests)
- [ ] Test binary count reduced from 12 to 6
- [ ] Before/after `cargo test --no-run` timing in PR description
- [ ] CI scripts checked for hardcoded paths to moved files

---

### VIO-637: Adopt cargo-nextest — RECOMMENDED THIRD

**Impact: Low-Medium | Effort: Low | Risk: Low**

**Why it matters:** cargo-nextest runs each test binary's tests in parallel processes, provides better output formatting, retries flaky tests, and produces JUnit XML for CI. The main win is in CI where test execution is longer.

**Current state:** 912 tests across 12 binaries, `cargo test` takes 8.3s incremental. After VIO-638, this drops to 6 binaries — nextest would run all 6 in parallel.

**Approach:**
1. Install: `cargo install cargo-nextest` (or `cargo binstall cargo-nextest`)
2. Add `.config/nextest.toml` for configuration (retry count, test grouping, output format)
3. Update `scripts/ci_rust.sh` to use `cargo nextest run` instead of `cargo test` inside `cargo llvm-cov`
4. Update `.claude/hooks/after-edit.sh` if it runs tests
5. Update CI workflow for JUnit output (`--message-format libtest-json`)

**Note on llvm-cov compatibility:** `cargo llvm-cov` supports nextest natively via `cargo llvm-cov nextest`. This is the recommended integration path.

**Expected gains:** 10-30% faster test execution in CI. Better failure output. Retry support for flaky tests. Marginal improvement locally since tests already run in 8.3s.

**Acceptance Criteria:**
- [ ] `cargo nextest run` passes all 912 tests
- [ ] `scripts/ci_rust.sh` uses `cargo llvm-cov nextest`
- [ ] `.config/nextest.toml` configured with sensible defaults
- [ ] CI produces JUnit XML test reports
- [ ] Local dev workflow documented (optional nextest install)

---

### VIO-636: Split sim_core into Smaller Crates — DEFER / NEEDS DESIGN

**Impact: Medium | Effort: High | Risk: High**

**Why I recommend deferring this:**

1. **The coupling is extreme.** sim_core exports 110+ public types used across 5 downstream crates. Any split requires a `sim_core_types` crate that everything depends on — and changes to types (the most common change) still trigger full workspace rebuilds.

2. **The bottleneck is LLVM codegen, not Rust compilation.** sim_core spends 82% of its 14.3s build time in LLVM codegen. Splitting into sub-crates helps parallelize codegen, but the gain depends on how evenly the code splits — and the types module is the most coupled part.

3. **The other 3 tickets get most of the wins.** Faster linker (VIO-635) + fewer test binaries (VIO-638) + parallel test execution (VIO-637) will likely cut `cargo test --no-run` from 55.8s to ~35-40s. That's a 30% improvement for a fraction of the effort.

4. **The risk is high.** Splitting a 35K LoC crate with 110+ exported types is a multi-day effort that will touch every file in the workspace. It creates merge conflicts with all in-flight work.

**If pursued later, the most viable split:**

```
sim_core_types (types/, state, content definitions)  ~5,000 LoC
    ↑
sim_core (tick logic, modules, engine)               ~30,000 LoC
    ↑
sim_control, sim_world, sim_cli, sim_daemon, sim_bench
```

This splits the "rarely changes" types from the "frequently changes" engine logic. But it only helps incremental builds when engine code changes — type changes still cascade.

**Recommendation:** Revisit after VIO-635/637/638 are done and you have new baseline measurements. If clean test compilation is still >40s, design the split more carefully with a dedicated brainstorm.

---

## Recommended Execution Order

```
VIO-635 (linker)     ──→ VIO-638 (test migration) ──→ VIO-637 (nextest)
   ~2 hours                  ~3 hours                    ~1 hour
   Quick win, biggest        Reduces link count          Drop-in improvement
   bang for buck             from 12 → 6                 on top of fewer binaries

                             VIO-636 (crate split) — DEFER
                             Revisit with new baseline measurements
```

VIO-635 should be first because the linker improvement benefits everything downstream — including the test binary links that VIO-638 eliminates and VIO-637 parallelizes.

## Expected Combined Impact

| Metric | Before | After (635+637+638) | Reduction |
|--------|--------|---------------------|-----------|
| Clean `cargo test --no-run` | 55.8s | ~35-40s | ~30-40% |
| Test binaries linked | 12 | 6 | 50% |
| Binary link time | 9.0s | ~3-5s | ~45-55% |
| Incremental test run | 8.3s | ~5-6s | ~25-35% |

## Sources

### Internal References
- Workspace structure: `Cargo.toml` (root)
- Build timings: `cargo build --timings` output (measured this session)
- Test organization: `crates/*/tests/` and `crates/*/src/tests/`
- CI pipeline: `scripts/ci_rust.sh`, `.github/workflows/ci.yml`
- Past learnings: `docs/solutions/patterns/batch-code-quality-refactoring.md` (module split gotchas, hardcoded CI paths)
