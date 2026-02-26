# PR Review Checklist

Mandatory checklist for Claude Code PR reviews. Every item must be checked before posting a review. Call out specific issues with `file:line` references. Fix issues before merging — do NOT note issues then merge anyway.

## Review Process

1. **Read the PR diff thoroughly** — `gh pr diff N`, not just the summary
2. **Understand the intent** — what problem does this solve? Is the approach sound?
3. **Check every item below** — skip nothing, even for "simple" PRs
4. **Post review** — start with "Claude Code Review -- No issues found." or "Claude Code Review -- Issues found:"
5. **Fix before merge** — Critical and Important issues block merge. Minor issues get follow-up tickets.

## Checklist

### Correctness
- [ ] Edge cases handled (empty collections, zero values, None/null)
- [ ] Off-by-one errors in loops, ranges, slicing
- [ ] Panic paths: `.unwrap()` on values that could be `None`/`Err` in production
- [ ] Integer overflow on arithmetic (especially `u64` tick math)
- [ ] Logic errors in conditionals (negation, short-circuit evaluation)
- [ ] Race conditions in concurrent code (atomics, locks, async)

### Types & Data
- [ ] No duplicate types across modules (same struct defined in two places)
- [ ] No unnecessary type conversions (e.g., `as` casts that lose precision)
- [ ] Field types match their usage (u32 vs u64, f32 vs f64)
- [ ] Serialization: `#[serde(rename)]` and field names match API contracts

### Error Handling
- [ ] `Result` types handled, not silently discarded
- [ ] Error responses include useful context (not just "error")
- [ ] No swallowed errors (`.ok()`, `let _ = ...` on Results that matter)
- [ ] Propagation: errors bubble up to appropriate handler, not caught too early

### Async Safety (Rust)
- [ ] No blocking calls in async handlers (synchronous file I/O, heavy computation)
- [ ] Lock scope minimized — no `.lock()` held across `.await` points
- [ ] Channels: bounded channels preferred, unbounded justified if used
- [ ] Spawned tasks handle their own errors (don't silently drop panics)

### API Design
- [ ] Response shapes consistent with existing endpoints
- [ ] HTTP status codes correct (200 vs 201 vs 204, 400 vs 404 vs 422)
- [ ] New endpoints documented in `docs/reference.md`
- [ ] Breaking changes flagged (field renames, removed endpoints)

### Test Coverage
- [ ] New code has tests — not just happy path
- [ ] Edge cases tested (empty input, boundary values, error conditions)
- [ ] Assertions are specific (not just `assert!(result.is_ok())`)
- [ ] Test names describe the behavior being verified
- [ ] No flaky patterns (timing-dependent assertions, uncontrolled randomness)

### Security
- [ ] User input validated at system boundaries
- [ ] No path traversal (file paths from user input)
- [ ] No injection vectors (SQL, command, template)
- [ ] Secrets not logged or included in error messages

### Rust-Specific
- [ ] No unnecessary `.clone()` — borrow where possible
- [ ] Clippy-worthy patterns fixed (manual implementations of std methods, etc.)
- [ ] Lifetime annotations correct and minimal
- [ ] `pub` visibility justified — prefer `pub(crate)` or private

### TypeScript-Specific
- [ ] No `any` types — use proper typing or `unknown`
- [ ] Promises handled (no unhandled rejections, missing `.catch()`)
- [ ] Null/undefined checks where values could be absent
- [ ] Event listeners cleaned up (useEffect return, removeEventListener)

### Performance
- [ ] No O(n^2) or worse in hot paths (tick loop, per-frame rendering)
- [ ] Allocations minimized in tight loops (pre-allocate, reuse buffers)
- [ ] Database/API calls not inside loops
- [ ] Large collections: HashMap/HashSet over linear search when appropriate

### Documentation & Style
- [ ] Changed public APIs have updated docs
- [ ] Complex logic has comments explaining *why*, not *what*
- [ ] No dead code committed (commented-out blocks, unused imports)
- [ ] File organization follows existing project conventions

## Severity Levels

| Level | Action | Examples |
|-------|--------|---------|
| **Critical** | Block merge, fix immediately | Data loss, security vulnerability, panic in production path |
| **Important** | Block merge, fix before proceeding | Missing error handling, untested edge case, race condition |
| **Minor** | Create follow-up ticket | Style inconsistency, missing doc comment, suboptimal but correct code |
| **Note** | Informational only | Suggestion for future improvement, alternative approach worth considering |

## Anti-Patterns in Reviews

- **Summarizing code** — describe *issues*, not what the code does
- **Rubber-stamping** — "LGTM" without checking the list is not a review
- **Nitpicking style** — if it passes linters, don't argue about formatting
- **Noting issues then merging** — if you found a bug, fix it first
- **Reviewing only the latest commit** — review ALL commits in the PR
- **Ignoring test quality** — tests that don't assert meaningful behavior are worse than no tests
