# Claude Execution Plan

## Scope

Complete exactly the first incomplete task listed in `TODO.md`, using `TODO.md` as the authoritative task source. Do not proceed to the next task after completion.

## Step-by-step plan

1. Read `TODO.md` to identify the first task whose heading is not prefixed with `[DONE]`.
2. Inspect the latest commit only for directly relevant unfinished notes tied to that selected task.
3. Read the selected task details, dependencies, validation requirements, and nearby plan context as needed.
4. Implement the selected task completely, avoiding workarounds or scope narrowing.
5. Run `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, then the relevant/full test suite required by the task.
6. If a blocking prerequisite or unscheduled failing test is found, update `TODO.md` with the minimum required prerequisite task, commit that bookkeeping, and stop.
7. If the task is completed, mark its title in `TODO.md` with `[DONE]`, update its completion record, and update this plan file with completion progress.
8. Commit all changes for this invocation with a clear task-specific message and the required co-author trailer.

## Progress

- Initialized execution plan.
- Selected first incomplete task: `M7.R Review`.
- Review focus: real/mock provider branching, incremental streaming, real tool loop termination, request cancellation and branch token handling, error mapping/status display, network dependency boundaries, and default non-network validation.
- Reviewed implementation and focused regression coverage for M7. No blocking issue found; proceeding to required fmt, clippy, and test validation.
- Validation completed successfully: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, and `cargo fmt --all -- --check`.
- Marked `M7.R Review` as `[DONE]` in `TODO.md` with the review completion record.
