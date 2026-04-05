---
title: P5 Station Construction patterns — long autonomous multi-ticket session
category: patterns
date: 2026-04-05
tags:
  - multi-ticket
  - stacked-prs
  - parallel-sessions
  - rust
  - serde
  - station-frames
  - workflow
  - project-implementation
related:
  - patterns/multi-ticket-satellite-system-implementation.md
  - patterns/crew-system-multi-ticket-implementation.md
  - patterns/multi-epic-project-execution.md
  - patterns/p3-tech-tree-expansion-patterns.md
  - patterns/stat-modifier-tech-expansion.md
---

# P5 Station Construction — Patterns from a 14-ticket autonomous session

This doc captures the reusable lessons from the P5 Station Construction & Multi-Station implementation session (2026-04-05). It is deliberately broader than the other `multi-ticket-*` pattern docs because the session exercised **workflow**, **codemod**, and **domain design** patterns simultaneously and at high volume.

Fourteen P5 tickets landed in a single ~10.5-hour session, roughly 45 minutes per ticket including pr-reviewer cycles, all while a second Claude Code session worked P3 Tech Tree Expansion in parallel on the same `main`. The scope included the full Station Frame+Slot system (SF-01 through SF-07) plus the Station Construction pipeline (VIO-590 through VIO-594).

If you only take away three things from this doc:

1. **`#[serde(default)]` does nothing for struct literals** — extending a widely-used Rust type still requires patching every literal initializer. Write a codemod, don't edit by hand.
2. **Stacked PRs auto-close when their base branch deletes on merge.** Have a one-command recovery loop ready. Expect to run it 7+ times in a 14-ticket session.
3. **CI merges your branch into *current* main before compiling** — a parallel session's new tests can fail your CI even when local passes. Rebase + forward-propagate fixture patches on every CI surprise.

The doc is organized into three parts: **(A)** the workflow / session mechanics, **(B)** the Rust codemod pattern for extending widely-constructed types, and **(C)** the domain design patterns used inside the P5 codebase itself.

---

## Part A — Session workflow: stacked PRs, parallel sessions, CI surprises

### What shipped (and the cadence)

Fourteen P5 tickets merged back-to-back via `/implement` + `/project-implementation`:

| Group | Tickets | Scope |
|---|---|---|
| SF-series (data model) | VIO-490..496 | `FrameId`, `FrameDef`, `StationState.frame_id`, `ModuleState.slot_index`, `ModifierSource::Frame`, slot validation, API/FE exposure |
| Construction layer | VIO-590..594 | Construction vessel hull + recipe + `tech_station_construction`, 3 station kits, `Command::DeployStation`, `TaskKind::ConstructStation`, lifecycle events, kit seed supplies |

PRs ran from #442 through #470. Of the 14 merged tickets, **7 required PR replacement** because their stacked base PR merged and deleted its branch before the downstream could merge. Timestamps show a stacked cadence: implementer work on N+1 while pr-reviewer runs on N and CI runs on N-1.

### Pattern A1 — The stacked PR cascade and its recovery

**Core problem.** When a ticket series has hard dependencies (SF-01 defines types SF-02 uses, SF-02 defines content SF-03 loads, etc.), `/implement`'s "branch from main" rule only works for the first ticket. Everything else must stack with `gh pr create --base feat/<predecessor>`.

Stacked PRs are time bombs. When the base PR squash-merges with `--delete-branch` (the `/implement` default), GitHub:

1. Deletes the base ref from origin.
2. Auto-closes every PR whose base ref was that branch.
3. The closed PR is **not reopenable**. Its commits are still on the head ref, but the PR record is gone.

The downstream branch itself survives — the commits still exist locally and on origin. Only the *PR record* is destroyed, which is why recovery is possible.

**Recovery recipe** — run this whenever `gh pr view <num>` returns `CLOSED` with no merge commit on a stacked PR:

```bash
# 1. Refresh local view of main
git fetch origin
git checkout main && git pull

# 2. Rebase the stacked branch onto fresh main
git checkout feat/<ticket>-<slug>
git rebase origin/main
# (resolve conflicts — see Pattern A3 below)
cargo fmt --all
cargo check --workspace

# 3. Force-push the rebased branch (always --force-with-lease)
git push --force-with-lease

# 4. Create a new PR targeting main
gh pr create --base main \
  --title "<ticket-id>: <title>" \
  --body-file /tmp/wt-pr-body.md

# 5. Update the Linear ticket attachment
# mcp__linear-server__save_issue(id=..., links=[{url, title}])
```

First time this costs ~15 minutes because it looks like a GitHub bug. With the recipe cached, ~5 minutes per cascade.

**When to stack vs. serialize:**

- **Serialize** (wait for predecessor to fully merge before branching): Zero cascade risk. Costs ~5–10 min dead time per ticket × 14 = 1–2 hours of wall-clock slack over the session.
- **Stack** (start next ticket immediately with `--base feat/<predecessor>`): No dead time, but every auto-close costs ~5 min recovery. For this session, 7 cascades × 5 min = ~35 min recovery cost.

Stacking wins on raw throughput **if** the recovery pattern is cached. For cold agents with no memory of the pattern, serialize. The current `/implement` and `/project-implementation` docs assume serialization; stacking is a power-user deviation.

### Pattern A2 — Parallel-session coordination

A second Claude Code session worked the P3 Tech Tree Expansion project out of `.claude/worktrees/wtp4` concurrently. Both sessions merged into the same `main`. Git log from the session shows interleaved P5 and P3 commits landing within the same window.

**Coordination rules that worked:**

1. **File-level disjoint ownership.** P5 owned `crates/sim_core/src/station/`, `types/content.rs::FrameDef`, `content/frame_defs.json`. P3 owned `crates/sim_core/src/research.rs`, `DataKind`/`ResearchDomain` constants, `content/techs.json`. Both touched `lib.rs` exports and `theme.ts` — those overlaps were pure additions and rebase-resolved.

2. **Always `git fetch origin main && git rebase origin/main` *immediately before* `gh pr create`**, not just at branch-start. Main drifts every 10–30 minutes during an active parallel session.

3. **Bulk-edit scripts must exclude `.claude/worktrees/`.** This is a hard, non-negotiable rule because it crosses session boundaries and can corrupt another agent's uncommitted work. In this session a Python codemod used `Path.rglob("crates/**/*.rs")` and walked into `.claude/worktrees/wtp4/crates/`, modifying the parallel session's files. Recovery: `git -C .claude/worktrees/wtp4 checkout -- crates/`. Always use `Path.glob("crates/*/src/**/*.rs")` or explicitly filter: `if ".claude/worktrees" in str(path): continue`.

4. **System-reminder "file has been modified" mid-edit → re-read before editing.** When the P3 session added `DataKind::ENGINEERING` into `types/mod.rs` between my `Read` and my intended `Edit`, Claude Code's file-modified guard fired. **Correct response: stop, re-`Read` the file, re-locate the insertion point, then `Edit`.** Never retry the stale `old_string` — it either fails or matches the wrong location.

5. **Linear attachment hygiene for stacked/replacement PRs.** Linear attachments persist when PRs close. After a replacement PR lands, the ticket shows both the closed original and the open replacement. Options: `mcp__linear-server__delete_attachment` on the stale one, or just append the new one via `save_issue(links=[...])` and let a human reconcile. In rapid-flow work I appended; the Linear UI shows both and the PR status badge disambiguates.

### Pattern A3 — CI surprise from parallel merges + fixture propagation

**The problem.** CI on a branch doesn't compile the branch in isolation — GitHub Actions merges the PR head into its base target and compiles *that*. If main was updated by a parallel session between your last local test run and CI, new code can appear that your branch didn't know about.

**Concrete instance from this session.** SF-01 added `slot_index: Option<usize>` to `ModuleState` and patched every call site on the SF-01 branch. Meanwhile P3 merged VIO-588/587/compound docs into main, including new test files in `crates/sim_core/src/station/lab.rs` with `ModuleState { ... }` literals. CI's merged-with-main build surfaced:

```
error[E0063]: missing field `slot_index` in initializer of `state::ModuleState`
   --> crates/sim_core/src/station/lab.rs:582
```

Tests that didn't exist when SF-01 branched.

**The fixture-propagation recovery pattern:**

```bash
# 1. Rebase onto fresh origin/main so the parallel session's new files
#    are in your tree.
git fetch origin
git rebase origin/main

# 2. Re-run your fixture codemod against the new files. For one-off
#    field additions, the same Python script from the original patch
#    can rerun harmlessly — it skips literals that already have the field.
python3 /tmp/wt-add-field.py

# 3. cargo fmt + cargo check + nextest on the affected crate
cargo fmt --all
cargo check --workspace
cargo nextest run -p sim_core

# 4. Amend the existing fixup commit (or add a new fixup commit if the
#    propagation is substantial enough to deserve its own message).
git add -u && git commit --amend --no-edit
git push --force-with-lease
```

Expect this loop to fire 1–3 times per long session. Keep the codemod around — don't delete it after the first success.

### Pattern A4 — Conflict resolution during stacked-PR rebases

When rebasing a stacked branch onto its updated base, you frequently hit conflicts where both branches applied *the same* fixture patch (e.g., both added `deploys_frame: None` to a `ComponentDef` literal). Git shows a conflict because the fixture appears twice in the graph, but semantically the two edits are identical.

**Resolution:**

```bash
# Take the upstream (base) version wholesale. Main already has the fix.
git checkout --ours -- <conflicted_file_1> <conflicted_file_2> ...
git add <conflicted_file_1> <conflicted_file_2> ...
cargo fmt --all    # the two patches may differ only in trailing-comma/whitespace
git rebase --continue
```

Never try to manually merge the conflict for pure-fixture files — you'll introduce whitespace noise that diverges from main.

**Escape hatch when rebase corrupts state.** A few times during this session a rebase went sideways (interrupted rebase, divergent formatting producing tangled conflicts). The reliable escape:

```bash
git rebase --abort
git reset --hard origin/main            # snap back to a known-good point
git cherry-pick <your-implementation-commit>   # replay only your real work
# resolve, fmt, test, force-push
```

**Rule of thumb:** If `git status` during a rebase looks confusing for more than 60 seconds, abort and replay via cherry-pick. Don't try to rescue it in place.

---

## Part B — Rust codemod pattern: extending widely-constructed types

### The core lesson

**`#[serde(default)]` is a deserialization-time default, not a Rust language default.** It fires only when a field is missing from the input JSON/bincode. It does nothing for:

- Struct literal initializers: `ModuleState { id, def_id, ..., /* must list slot_index */ }`
- Test fixtures constructing values directly
- Any code using `StructName { ... }` rather than `serde_json::from_str(...)`

Adding a new `#[serde(default)]` field is **schema-compatible for persisted state** but **source-breaking for every struct literal in the crate**. In this session:

- `ModuleState.slot_index: Option<usize>` broke ~94 call sites across sim_core + sim_control
- `StationState.frame_id: Option<FrameId>` broke ~21 station literals
- `ComponentDef.deploys_frame` (VIO-591), then `deploys_seed_materials` + `deploys_seed_components` (VIO-594) broke ~20 content-loader test literals — twice, on consecutive tickets

### Pattern B1 — Codemod recipe for adding a default field

1. **Add the field to the struct definition with `#[serde(default)]`.** Plus a serde backward-compat test that round-trips a minimal JSON missing the new field.
2. **Add the field to the builder helper** in `test_fixtures.rs` (see Pattern B3 — this is where blast radius gets cut from ~100 sites to the builder plus maybe 10 raw-literal stragglers).
3. **`cargo check -p sim_core 2>&1 | grep "missing field"`** — this gives you the exact file:line of every remaining literal that needs patching. The compiler is your worklist generator.
4. **Write a brace-balanced Python codemod** — see Pattern B2 for the shape. Store it at `/tmp/wt-add-field.py` following the `wt-` /tmp convention.
5. **Run the codemod.** It should rewrite N files in seconds.
6. **`cargo fmt --all`** — always. Codemods leave inconsistent trailing commas and whitespace that CI format check will reject.
7. **`cargo check --workspace`** — any remaining errors are usually `..base_expr` functional-update cases the codemod correctly skipped and that already compile, or macro invocations the codemod didn't see. Fix those by hand.
8. **`cargo nextest run -p sim_core -p sim_control`** to confirm no semantic regressions.
9. **Commit as one logical unit** — don't split "add field to struct" from "update N test fixtures". Reviewers need to see the whole change.

### Pattern B2 — The brace-balanced codemod with guards

A naïve regex like `StructName\s*\{[^}]*\}` breaks on any literal with nested braces (struct literals within struct literals). The pattern that works:

```python
# Pseudocode — see /tmp/wt-add-field.py templates from the session
def find_struct_literals(text, struct_name):
    """Return (open_idx, close_idx) ranges for literals only."""
    i = 0
    while True:
        pos = text.find(struct_name, i)
        if pos < 0: break

        # Guard 1: preceding token must not be '->', 'impl', 'struct', 'enum'
        # These mean function return type, impl block, type definition.
        preceding = text[max(0, pos-40):pos].rstrip()
        if preceding.endswith('->') or \
           re.search(r'\bimpl\b[^{;]*$', preceding) or \
           preceding.endswith(('struct', 'enum')):
            i = pos + len(struct_name)
            continue

        # Guard 2: next char after name must not be '::' (that's a path/method
        # call like StructName::method). But 'crate::StructName' IS a literal —
        # the '::' is before the name, not after.
        end = pos + len(struct_name)
        if end < len(text) and text[end] == ':' and text[end+1] == ':':
            i = end
            continue

        # Guard 3: after whitespace the next char must be '{'
        j = end
        while j < len(text) and text[j] in ' \t\n': j += 1
        if j >= len(text) or text[j] != '{':
            i = end
            continue

        # Brace-balanced scan, respecting string literals and line comments
        depth = 0
        k = j
        while k < len(text):
            c = text[k]
            if c == '{': depth += 1
            elif c == '}':
                depth -= 1
                if depth == 0:
                    yield (j, k)
                    break
            # TODO: also skip inside "..." strings and // comments
            k += 1
        i = k + 1
```

Then when inserting, check the body for `..base_expr` and **skip the patch** if present — functional record update cannot have additional fields after the base.

### Pattern B3 — Builder helpers are the blast-radius lever

The test fixture `test_module()` in `crates/sim_core/src/test_fixtures.rs`:

```rust
pub fn test_module(def_id: &str, kind_state: ModuleKindState) -> ModuleState {
    ModuleState {
        id: ModuleInstanceId(format!("{def_id}_instance")),
        def_id: def_id.to_string(),
        enabled: true,
        kind_state,
        wear: WearState::default(),
        thermal: None,
        slot_index: None,        // <- added once, inherited by every caller
        power_stalled: false,
        module_priority: 0,
        assigned_crew: Default::default(),
        efficiency: 1.0,
        prev_crew_satisfied: true,
    }
}
```

Every test using `..test_module("refinery", Processor(...))` inherits the new field for free — zero patches at those sites. Only raw `ModuleState { ... }` literals (ones that predated the helper or inlined the struct for some reason) need patching.

**The rule:** If a struct is constructed in more than ~5 test files, add a builder helper in `test_fixtures.rs` *before* the first field addition. Then every future field addition is a **1-line change** plus a handful of raw-literal stragglers.

### Pattern B4 — Gotchas hit in this session (keep this list nearby)

1. **Function return types look like struct literals.** `fn build() -> ModuleState {` matches the naïve regex and the `{` opens the *function body*, not a literal. Adding the new field inside the function body produces gibberish. Guard: lookbehind for `->`.

2. **`..base_expr` functional update is exclusive.** In `ModuleState { thermal: Some(...), ..test_module(...) }` you cannot add a new field after the base — the compiler rejects it. Adding it *before* the base is redundant since `test_module()` already supplies it. Detect `..<ident>` at the end of the body and skip patching.

3. **`crate::ModuleState` vs `ModuleState::method()`.** Both contain `::` near the name. The first is a qualified path to a literal; the second is an associated function call. Disambiguate by looking at the character *after* the identifier: `::` followed by a lowercase ident = method call; `{` (possibly after whitespace) = literal.

4. **`rglob("crates/**/*.rs")` walks into parallel worktrees.** The script will happily patch `.claude/worktrees/wtp4/crates/` files and corrupt the other session. **Always** filter `if ".claude/worktrees" in str(path): continue` or use the explicit glob `Path.glob("crates/*/src/**/*.rs")`.

5. **`cargo fmt` before CI.** Brace-balanced text surgery leaves inconsistent trailing-comma and whitespace style. CI's format check fails even when the code compiles. Run `cargo fmt --all` as step one of the validation phase.

6. **`sim_core::` vs `crate::` in internal tests.** Tests inside `sim_core` must use `crate::FrameId`, not `sim_core::FrameId`. The latter only works from sibling crates. I hit this in VIO-592 where I wrote test code as if it were in sim_world; had to sed-replace `sim_core::` → `crate::` across the new test block.

7. **`execute_at_tick` staleness in multi-tick tests.** When a test runs two `tick()` calls with the same command envelope, the second command won't execute because `execute_at_tick` was bound to the original tick. Use `CommandEnvelope { execute_at_tick: state.meta.tick, ... }` **per tick**, rebuilding the envelope in a helper.

---

## Part C — Domain design patterns from the P5 code itself

### Pattern C1 — Mirror pattern: Frame ↔ Hull

The entire station frame system is a structural mirror of the ship hull system:

| Ship side | Station side |
|---|---|
| `HullId` newtype | `FrameId` newtype |
| `HullDef { cargo, slots, bonuses, required_tech }` | `FrameDef { cargo, slots, bonuses, required_tech }` |
| `ShipState.hull_id: HullId` | `StationState.frame_id: Option<FrameId>` |
| `ModifierSource::Hull(HullId)` | `ModifierSource::Frame(FrameId)` |
| `recompute_ship_stats(ship, content)` | `recompute_station_stats(station, content)` |

```rust
// commands.rs — recompute_station_stats mirrors recompute_ship_stats
pub fn recompute_station_stats(station: &mut StationState, content: &GameContent) {
    station.core.modifiers
        .remove_where(|s| matches!(s, ModifierSource::Frame(_)));
    let Some(frame_id) = station.frame_id.clone() else { return; };
    let Some(frame) = content.frames.get(&frame_id) else { return; };
    for bonus in &frame.bonuses {
        let mut modifier = bonus.clone();
        modifier.source = ModifierSource::Frame(frame_id.clone());
        station.core.modifiers.add(modifier);
    }
    station.core.cargo_capacity_m3 = station.core.modifiers
        .resolve_f32(StatId::CargoCapacity, frame.base_cargo_capacity_m3);
}
```

**Why mirror?** Ships and stations are both "base chassis with slots + stat bonuses". Inventing a parallel vocabulary would double the code reviewers need to learn. Same field names, same `ModifierSource` shape, same recompute recipe → zero new mental model.

**One deliberate asymmetry:** `StationState.frame_id: Option<FrameId>` where `ShipState.hull_id: HullId` is non-optional. Stations can be frameless (legacy saves, P4 ground-launched shells before P5 landed). The recompute handles this with an early-return that **also clears stale frame modifiers** — de-framing a station is well-defined. The test `recompute_station_stats_clears_stale_frame_modifiers_when_unframed` guards this ordering.

**Reusable when:** Adding a new "chassis-like" entity (future: facility template, fleet flagship, orbital ring). Copy hull verbatim; the modifier pipeline already handles multi-source resolution.

**Caveat:** Keep the asymmetry's direction consistent — if you make a field optional on the new entity, make sure the clear-then-reapply ordering is preserved.

### Pattern C2 — Transit→X chained task for atomic "go do thing"

`TaskKind::Transit` is a recursive enum: it owns a boxed follow-on task that starts the moment transit resolves.

```rust
// types/state.rs
Transit {
    destination: Position,
    total_ticks: u64,
    then: Box<TaskKind>,
}

ConstructStation {
    frame_id: FrameId,
    position: Position,
    assembly_ticks: u64,
    kit_component_id: String,   // consumed at command time; kept for completion event
}
```

`handle_deploy_station` builds `Transit { then: Box<ConstructStation> }` with a co-located fast path that skips Transit entirely:

```rust
let construct_task = TaskKind::ConstructStation { /* ... */ };
let (final_task, duration) = if travel_ticks == 0 {
    (construct_task, construct_task.duration(&content.constants))
} else {
    let transit = TaskKind::Transit {
        destination: target_position.clone(),
        total_ticks: travel_ticks,
        then: Box::new(construct_task),
    };
    (transit, transit.duration(&content.constants))
};
```

`resolve_transit` picks up the `then` in one step and emits a specialized event when the follow-on is `ConstructStation`:

```rust
// tasks.rs — at end of resolve_transit
let duration = then.duration(&content.constants);
ship.task = Some(TaskState { kind: then.clone(), started_tick: current_tick,
                             eta_tick: current_tick + duration });
events.push(TaskStarted { /* ... */ });
if let TaskKind::ConstructStation { frame_id, position, assembly_ticks, .. } = then {
    events.push(StationConstructionStarted { /* ... */ });
}
```

**Why chosen over alternatives:**
- **Not a separate command queue.** Ship stays under a single `TaskState` throughout; save/load, event emission, and autopilot "what's this ship doing?" queries use existing paths.
- **Not two sequential commands.** A two-step path (`AssignTask(Transit)` → wait → `AssignTask(ConstructStation)`) would race against autopilot re-planning and fail to atomically reserve the kit.
- **Not an `ArrivalHook` closure.** Closures can't `Serialize`/`Deserialize` — save files would break.

**Reusable when:** Semantic is "go here, then do a specific thing atomically at arrival". Candidates: `Transit→Dock`, `Transit→Mine`, `Transit→Refuel`.

**Caveats:**
1. `Box<TaskKind>` adds 8 bytes to every TaskKind variant. Acceptable for ships (not hot-loop data); watch it on per-frame structs.
2. The co-located fast path **must** be tested independently. Otherwise you get the "I built the chain correctly but only exercised transit mode" bug.
3. Don't nest `Transit { then: Transit { ... } }` — legal but a code smell. Use intermediate variants.

### Pattern C3 — Content-side binding: `deploys_frame` on ComponentDef

Instead of an external `kit_frame_map.json` or a hardcoded Rust match, each kit's `ComponentDef` declares its own target frame inline:

```rust
// types/content.rs
pub struct ComponentDef {
    pub id: String,
    pub name: String,
    pub mass_kg: f32,
    pub volume_m3: f32,
    #[serde(default)] pub deploys_frame: Option<FrameId>,
    #[serde(default)] pub deploys_seed_materials: Vec<InitialMaterial>,
    #[serde(default)] pub deploys_seed_components: Vec<InitialComponent>,
}
```

`handle_deploy_station` reads the binding directly:

```rust
fn validate_deploy_inputs(...) -> Option<DeployKitInfo> {
    let InventoryItem::Component { component_id, .. } = ship.inventory.get(kit_item_index)? else { return None; };
    let kit_def = content.component_defs.iter().find(|c| c.id == component_id.0)?;
    let frame_id = kit_def.deploys_frame.clone()?;    // the binding
    if !content.frames.contains_key(&frame_id) { return None; }
    Some(DeployKitInfo { frame_id, /* ... */ })
}
```

**Why chosen:**
- vs. `kit_frame_map.json`: two files to edit when adding a kit, two files that can disagree, no single source of truth.
- vs. hardcoded Rust match: content authors can't add kits without editing engine code. Violates CLAUDE.md's "data-driven content types" rule.
- vs. string-parsing the kit ID: conventions break silently when someone renames an ID.

**Reusable when:** A content type needs to reference another content type for a deterministic behavior. Keep the binding `Option<T>` so non-binding items of the same type (regular components like `repair_kit`, `thruster`) stay empty.

**Caveat:** Add content-loader validation (`sim_world::load_content`) that checks the referenced target exists, or runtime validation that degrades gracefully (this session's handler returns `None` — the command silently fails rather than panicking).

### Pattern C4 — Event extension via optional serde fields

`Event::StationDeployed` already existed from the P4 ground-launch system. VIO-592 extended it instead of creating a new variant:

```rust
StationDeployed {
    station_id: StationId,
    position: Position,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ship_id: Option<ShipId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    frame_id: Option<crate::FrameId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kit_component_id: Option<String>,
}
```

- **Load side:** old P4-era save files deserialize successfully — `default` yields `None`.
- **Save side:** events without the new fields don't write them; old-format save files byte-identical, new saves unbloated.
- P4 launches emit `StationDeployed { station_id, position, ship_id: None, frame_id: None, kit_component_id: None }`.
- P5 construction ships emit the same variant with all three `Some(_)`.

**Why chosen over alternatives:**
- vs. new `StationConstructedByShip` variant: FE and analytics would have two "station was deployed" events to handle. Event-sync CI would need two entries.
- vs. a `source: StationDeploySource` enum field: breaks all existing save files; needs a migration.
- vs. versioned events (`StationDeployedV2`): more code surface; UI discriminator still needed.

**Reusable when:** Extending an existing event with context-specific fields that older emitters cannot populate. Rule of thumb: if `None` means "not applicable here" (not "we don't know yet"), optional fields are the right call.

**Caveats:**
1. Consumers must handle both cases. FE `applyEvents` must not `.unwrap()` the new fields.
2. Don't stack optional fields indefinitely. After ~3 optional fields, the variant becomes a poorly-disguised union type — at that point, split or introduce a discriminator.
3. **Always pair `default` with `skip_serializing_if`.** `default` alone writes `"ship_id": null` into every old save, bloating log streams. `skip_serializing_if` alone breaks forward-load on missing fields.

### Pattern C5 — Slot validation centralization + autopilot pre-check

**The problem.** Two InstallModule commands in the same tick batch both target the "next free" slot. If validation lives only in the autopilot, both pass the pre-check and the second corrupts state on apply.

**The solution.** Handler is the source of truth; autopilot pre-check is an optimization, not a correctness layer.

```rust
enum SlotResolution {
    Frameless,           // legacy station, no frame — legal
    Slot(usize),         // validated index
    NoCompatibleSlot,    // framed but no fit — rejected with event
}

fn resolve_install_slot(station, def, requested_slot, content) -> SlotResolution {
    let Some(frame_id) = station.frame_id.as_ref() else { return SlotResolution::Frameless; };
    let Some(frame) = content.frames.get(frame_id) else { return SlotResolution::Frameless; };
    let occupied: HashSet<usize> = station.core.modules.iter()
        .filter_map(|m| m.slot_index).collect();
    // ... explicit-slot validation then auto-find first-fit
}
```

The 3-variant enum (not `Option<usize>`) makes the "frameless legacy path" vs "failed validation" distinction explicit — a frameless station passing `None` is legal; a framed station failing validation is an error with its own `ModuleNoCompatibleSlot` event.

SF-06 adds an autopilot pre-check that pre-claims slots in a batch, preventing two items from racing for the same slot. This is **two layers of defense**, not redundancy — the autopilot plans better, but the handler still enforces.

**Reusable when:** Any resource-allocation command (slot, crew assignment, docking bay, research queue) where batches could race. Give it a 3-variant outcome enum when "not applicable" differs meaningfully from "failed".

### Pattern C6 — Function-length decomposition (never `#[allow]`)

Both `handle_install_module` and `handle_deploy_station` exceeded clippy's `too_many_lines` threshold (100) after adding validation. Per memory rule 17, never `#[allow(clippy::too_many_lines)]` — decompose instead.

**The three shapes that worked:**

1. **Validation-with-failure-event helper** (like `tech_gate_passed`):
   ```rust
   fn tech_gate_passed(state, station_id, def, ..., events) -> bool {
       // Check required_tech. On fail: return module to inventory,
       // emit ModuleAwaitingTech, return false.
   }
   ```
   Called with `if !tech_gate_passed(...) { return false; }` — keeps the handler's happy path linear.

2. **Resource-consumption step** (like `consume_kit_from_ship`):
   ```rust
   fn consume_kit_from_ship(ship: &mut ShipState, kit_item_index: usize) -> bool
   ```
   Mutates state, returns bool for "did it work". Single `if !helper() { return false; }` in the handler.

3. **Resolved-metadata struct** (like `DeployKitInfo`):
   ```rust
   struct DeployKitInfo { frame_id, kit_component_id, assembly_ticks }
   fn validate_deploy_inputs(&state, &content, ship_id, idx) -> Option<DeployKitInfo>
   ```
   Named struct carrying validated data from a lookup-heavy read-only helper to the mutating builder step. Makes the borrow boundary explicit.

**Caveats:**
- **Don't over-extract.** A helper called from one site with no meaningful name beyond `step_3_of_handle_deploy_station` is worse than inline. Each helper needs a name that tells you *why* it exists.
- **Watch borrow patterns.** When `tech_gate_passed` takes `&mut GameState`, the caller must **re-borrow** the station afterward. Document this or it'll confuse the next reader.
- **Keep helpers `fn`, not `pub(crate)`**, unless cross-module reuse is real. Private helpers stay free to rename.

---

## Checklist for the next P5-scale autonomous session

Use this as a prep checklist before starting a multi-ticket sprint:

**Before branching:**
- [ ] Read the parallel session's project brief if one exists — know which files are theirs
- [ ] Identify high-overlap files (`lib.rs`, `theme.ts`, `initial_*.json`) and plan resolution strategy
- [ ] Decide stack vs. serialize based on the session's cascade-recovery muscle memory

**Before each PR:**
- [ ] `git fetch origin main && git rebase origin/main` *immediately before* `gh pr create` (not at branch-start)
- [ ] `cargo fmt --all && cargo check --workspace && cargo nextest run -p <affected crates>`
- [ ] `./scripts/ci_event_sync.sh` if you added or renamed any `Event` variants
- [ ] `gh pr create --base <base> --body-file /tmp/wt-pr-body.md` (never inline bodies)

**When adding an optional struct field:**
- [ ] Add to the struct with `#[serde(default)]`
- [ ] Add backward-compat deserialization test (missing-field case)
- [ ] Add roundtrip test (with-field case)
- [ ] Add to the builder helper in `test_fixtures.rs` **first** — this cuts blast radius
- [ ] Run `cargo check` to enumerate remaining literal sites
- [ ] Write/reuse a brace-balanced Python codemod with the 6 guards from Pattern B4
- [ ] `cargo fmt --all` after the codemod
- [ ] Commit the struct + fixtures together as one logical change

**When a stacked PR's base merges:**
- [ ] Don't panic — commits still exist on head ref
- [ ] Rebase onto fresh main, force-push with lease
- [ ] New `gh pr create --base main`
- [ ] Append new PR link to Linear ticket via `save_issue(links=[...])`

**When CI fails with "missing field" after a rebase:**
- [ ] Parallel session probably added new struct literals
- [ ] Rerun the original codemod — it's idempotent against already-patched files
- [ ] `cargo fmt --all`, amend, force-push

**When a rebase looks corrupted:**
- [ ] 60-second rule: abort if confused
- [ ] `git rebase --abort && git reset --hard origin/main && git cherry-pick <implementation-commit>`
- [ ] Faster than rescuing in place

---

## Throughput data point

For future session planning: **14 tickets in ~10.5 hours ≈ 45 min/ticket** with pr-reviewer cycles, in a hot state where the stacked-cascade recovery pattern was cached. Compare to earlier sessions: P3 tech tree ~9 tickets, P4 satellites ~5 tickets. The P5 cadence was sustainable because:

- Most SF-series tickets were narrow (single-file schema changes or validation additions)
- Codemod automation eliminated the "50 test fixtures to update by hand" time sink
- Parallel session overlap was minimal (disjoint ownership held)
- Construction layer tickets reused infrastructure from SF-series (no new abstractions per ticket)

**What deferred from the original P5 epic:** 11 tickets remain — logistics layer (VIO-595/596/598/599), ownership (VIO-486/488), polish (VIO-597/600..604). This is an intentional slice: the core deployment loop is shippable without the inter-station logistics. A future session should pick up with the logistics layer as its own self-contained block.

## See also

- **`patterns/multi-ticket-satellite-system-implementation.md`** — prior multi-ticket pattern doc. This P5 doc extends it with stacked-PR cascade recovery, parallel-session coordination, and the `#[serde(default)]` codemod gotcha.
- **`patterns/multi-epic-project-execution.md`** — covers sequential cross-epic obsolescence. Does not cover parallel sessions or stacked PRs; this doc adds both.
- **`patterns/p3-tech-tree-expansion-patterns.md`** — the parallel session's compound doc from the same day, covering tech tree work. Useful companion reference for what the other agent was doing.
- **`patterns/stat-modifier-tech-expansion.md`** — the `ModifierSource` pipeline Pattern C1 reuses.
- **`patterns/crew-system-multi-ticket-implementation.md`** — earlier multi-ticket bulk-field-addition precedent; this doc generalizes its fixture-propagation pattern.
