# Project Conventions (Normative)

This document defines the coding conventions for this repository.
Rules use RFC 2119 keywords (`MUST`, `SHOULD`, `MAY`).

If a change conflicts with this document, the change MUST be revised to
conform unless an explicit exception is documented in the same commit.

## 1. Architectural Boundaries

- `src/app.rs` MUST remain composition/wiring only.
- Command handlers in `src/cli/handlers/` SHOULD orchestrate flow and
  delegate domain logic to `src/cli/support/` or domain modules.
- Shared behavior used by multiple handlers MUST be extracted once and
  reused (do not duplicate call-site logic).

## 2. Function Signature Style (No Drift Rule)

### 2.1 Orchestration and Boundary APIs

- High-arity orchestration functions MUST use typed options structs.
- This applies to:
  - `handle_*` functions with many arguments.
  - `run_*_flow` functions coordinating multiple sub-steps.
  - preflight/setup entrypoints that combine booleans/paths/flags.
- Threshold: use an options struct when either condition is true:
  - 6+ parameters, or
  - 2+ boolean parameters.
- Options structs SHOULD be colocated near the function they configure
  and named `*Options`.

### 2.2 Low-Level Shared Helpers

- Low-level shared helpers MUST prefer explicit parameters.
- Do not introduce options structs for simple utility/shared helpers
  unless a true domain object is being modeled.
- If a low-level helper becomes hard to read, split behavior instead of
  hiding complexity in an unbounded options bag.

### 2.3 Consistency Requirement

- Within the same layer/module, signature style MUST be consistent.
- Refactors MUST NOT flip patterns back and forth without a documented
  architectural reason.

## 3. Module and File Shape

- New functionality MUST be added to the nearest domain module.
- Files SHOULD stay small and single-purpose.
- New types/functions SHOULD be placed in dedicated files when they are
  primary abstractions.
- New modules MUST be exported through local `mod.rs`/parent module
  consistently.

## 4. Error and Output Handling

- Repeated serialization/output formatting MUST be centralized.
- User-facing error messages SHOULD be consistent and actionable.
- Avoid ad-hoc `println!` format divergence when a shared renderer exists.

## 5. Refactor Discipline

- Refactors MUST preserve behavior unless the commit explicitly declares a
  behavior change.
- Broad refactors SHOULD reduce duplication, argument-order risk, and
  divergence between equivalent command flows.
- When a convention choice is made, subsequent refactors MUST align with
  it project-wide for touched code.

## 6. Pull Request / Commit Expectations

- Commits SHOULD be cohesive and scoped to one concern.
- Commit messages MUST explain why the refactor improves maintainability
  (not only what changed).
- Validation commands relevant to touched behavior MUST be run and
  reported (for Rust code, run `just build`).

