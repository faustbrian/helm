# Agent Instructions

- `CONVENTIONS.md` is the normative coding standard for this repo.
- You MUST read and follow `CONVENTIONS.md` before implementation tasks.
- You MUST apply `CONVENTIONS.md` consistently across touched files.
- You MUST NOT alternate between conflicting style patterns without a
  documented architectural reason.
- Signature style enforcement:
  - Orchestration/boundary APIs MUST use typed `*Options` structs when
    they have high argument count or multiple boolean flags.
  - Low-level shared helpers MUST prefer explicit parameters.
  - Do not introduce options-bag drift in utility layers.
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
