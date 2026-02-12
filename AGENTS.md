# Agent Instructions

- Run `just build` when `.rs` code files have been modified to verify the
  build still works.
- Keep `src/app.rs` focused on app wiring, initialization, and high-level orchestration.
- Add new functionality in the most relevant module directory (e.g. `src/git/remote/`, `src/git/tag/`) as small, single-purpose files.
- When introducing a new function or type:
  - Create a new file in the nearest domain folder.
  - Export it from that folder’s `mod.rs` (and re-export if needed).
  - Keep files small and scoped; prefer one primary function/type per file unless tightly coupled.
- If no suitable module exists, create a new submodule directory with a `mod.rs` and place the new file there.
- Avoid adding new helper functions to `src/app.rs` unless they are strictly app bootstrap or composition logic.
