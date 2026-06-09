## Execution Plan

1. Identify the first incomplete task in `TODO.md` using `[DONE]` prefixes as the only completion marker.
2. Review the selected task details, dependencies, validation requirements, and any directly relevant recent commit context.
3. Inspect only the code and tests needed for that task, avoiding unrelated historical triage.
4. Implement the selected task completely, or add the minimum prerequisite task if a concrete blocker prevents correct implementation.
5. Run formatting, linting, and relevant tests in the requested order; address any unscheduled failures that appear.
6. Update `TODO.md` with the `[DONE]` prefix and completion record if the task is completed, or record any prerequisite/blocker task if not.
7. Commit all task-related changes with a clear message and the required co-author trailer.
8. Stop after this single task.

## Progress

- Plan recorded before repository inspection.
- Selected first incomplete task: `#2b 同步命中测试`.
- Found titlebar rendering and hit testing already share `chrome::titlebar_layout`; remaining work is to add focused regressions for relocated close/maximize hit regions and titlebar drag behavior.
- Added regression coverage for relocated close/maximize/restore button hit regions and titlebar dragging outside the new button zones.
- Validation completed successfully: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all --all-targets`.
- Marked `#2b 同步命中测试` as `[DONE]` in `TODO.md`.
