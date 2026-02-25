# Save Button Feature Design

## Summary

Add a save button to the game UI that persists the current GameState to disk via the daemon.

## Backend

- New `POST /api/v1/save` endpoint in sim_daemon
- Locks sim state, serializes `GameState` to JSON, writes to `runs/<run_id>/saves/save_<tick>.json`
- Returns `{ "path": "<relative_path>", "tick": <N> }` on success
- Creates `saves/` subdirectory on first save

## Frontend

- Add `saveGame()` to `api.ts`
- Add Save button in the header/toolbar area
- Show brief inline feedback on success/error

## Out of Scope

- Save slots / save management UI
- Auto-save
- Load from UI (use `--state` CLI flag)
- Content version validation beyond existing checks
