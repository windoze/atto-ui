# Claude execution plan

## Current status
- Selected first incomplete task from TODO.md: **Docs 更新**.
- Latest commit is `[M7.R] Complete terminal config review`; it is directly relevant only as the last completed milestone, with no unfinished issue called out in the commit subject.
- `IMPLEMENTATION_PLAN.md` is not present at the repository root; the active phase plan is `PLAN.md`, so milestone-status updates will be made there.
- Documentation edits are complete: `TERMINAL_GAP.md`, `PLAN.md`, root `README.md`, new `crates/atto-ui-terminal/README.md`, and the `TODO.md` completion record.
- Diff whitespace check passed with `git diff --check`; validation commands were not rerun because this task changed documentation/progress files only.
- I will avoid recording private chain-of-thought; this file will contain an actionable execution plan and progress log.

## Step-by-step plan
1. Done: Update `TERMINAL_GAP.md` so it clearly records that P0-P3 terminal gaps are closed by M1-M7 and leaves no stale "missing" status as current truth.
2. Done: Update the root `README.md` with a terminal app quick start and link to terminal-specific docs.
3. Done: Add `crates/atto-ui-terminal/README.md` documenting the full terminal viewer feature set, shortcuts, configuration file behavior, and validation commands.
4. Done: Update `PLAN.md` milestone status to show M1-M7 complete and point the detailed completion record to `TODO.md`.
5. Done: Mark the `Docs 更新` task `[DONE]` in `TODO.md` and record that validation was skipped because this task changes documentation only after the previous full-suite green M7.R run.
6. Done: Review the Markdown diff for consistency.
7. Next: Commit the documentation/task updates with the required trailer.
