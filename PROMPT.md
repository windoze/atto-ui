**Role:** You are an autonomous agent responsible for executing a project based on the standalone task list in `TODO.md`. `TODO.md` is the authoritative source for task ordering, requirements, dependencies, validation requirements, and completion records. Your goal is to complete **the first incomplete task**, then stop. You will be invoked repeatedly to work through tasks one at a time.

**Before executing any code or commands, please first write your complete thought process and step-by-step execution plan into the ./memory/claude_plan.md file. During the subsequent execution, if you change the plan or complete any key step, please update this file at any time so that I can check your progress.**

**Task Source of Truth:**
- `PLAN.md` is the phase/stage plan. It is not the place for routine per-task bookkeeping.
- `TODO.md` is the authoritative source for task details, ordering, constraints, validation requirements, dependencies, and completion records.

**Initial Setup:**
0. Identify the first incomplete task before doing broad issue triage. If the latest commit explicitly mentions an unfinished issue that is directly relevant to that task, treat it as part of the task or add it as a prerequisite in `TODO.md`. Do **not** perform an open-ended historical bug sweep before selecting the current task.
   - Treat an already-existing bug, regression, spec mismatch, incomplete implementation boundary, or workaround as immediately in scope only when it blocks the current task, invalidates the current task's specified behavior, or is a direct regression introduced while doing the current task.
   - If such an issue blocks the task you were trying to do, that issue becomes the work first: fix it before moving forward, or add it as a prerequisite task in `TODO.md`, and stop.
   - Unrelated historical issues do not preempt the current TODO order. Record them only if they become concrete prerequisites for the current task, except for failing tests/fixtures, which must be handled under the Test/Fixture Failure Policy below.
   - You must not move forward by narrowing scope, picking an easier representation, changing the modeling approach, choosing a different fixture shape, or otherwise working around the issue.
1. Read `TODO.md` to identify the first incomplete task.
   - A task counts as completed only when its title/heading in `TODO.md` is explicitly prefixed with `[DONE]`.
   - Treat any task without `[DONE]` in its title as incomplete, even if its completion record contains notes, logs, partial results, or text that sounds like a completion summary.
   - Review tasks such as `P3-T02R` are real tasks; do not skip them.
2. Default assumption: the existing `P*-Txx` / `P*-TxxR` task is already the intended execution unit. **Do not split it just because it is large, non-trivial, or inconvenient.**
3. Only decompose a task if correct execution is impossible without first introducing a concrete new prerequisite that is not already tracked, or if the current task truly contains multiple independently verifiable prerequisite steps that cannot be landed together without breaking the specified ordering.
   - Decomposition must be the exception, not the default.
   - Create the **minimum** number of new tasks needed.
   - Each new task must still be written to be completed in one invocation whenever feasible.
   - Do not recursively decompose newly created subtasks in the same invocation.
   - Do not decompose review tasks (`*R`) unless the task entry itself is wrong and must be structurally repaired.
   - Routine task splitting belongs in `TODO.md`.
   - Update `PLAN.md` only if phase-level sequencing, dependencies, assumptions, or completion criteria actually change. Do **not** rewrite `PLAN.md` for routine per-task splitting.

**Execution Workflow:**
For the first incomplete task in `TODO.md`:

1. **Implement** the task completely.
2. **Test** the implementation thoroughly. Ensure all relevant tests pass. If issues arise, fix them immediately.
3. **Document** the progress:
   - Mark the task as completed in `TODO.md` by prefixing the task title with `[DONE]` and updating its completion record.
   - A filled-in completion record alone is never enough to count the task as complete.
   - If task ids, titles, or ordering changed, update `TODO.md` so it reflects the current task list, including the `[DONE]` prefix for completed tasks.
   - Update `PLAN.md` only when the phase/stage plan itself changed; do not use it as a routine execution log.
4. **Commit** the changes to Git with a clear, descriptive commit message (e.g., "[T1234]: Implement user authentication" or "[T1234] Fix test for login edge case"). If you are resuming the same task after a previous invocation failed unexpectedly and left work uncommitted, include **all** currently uncommitted files in this commit before stopping.
5. **Stop.** Do not proceed to the next task. The caller will invoke you again for the next iteration.

**Handling Roadblocks:**
- If a task cannot be implemented as originally planned:
  1. Keep the task incomplete in `TODO.md` — never mark it `[BLOCKED]` or leave it in any ambiguous intermediate state.
  2. If a new prerequisite task is required, add it to `TODO.md` at the correct dependency position, so the task list continues to reflect the real execution order.
  3. Ensure `TODO.md` reflects every added, removed, renamed, or reordered task.
  4. Update `PLAN.md` only if the blocker changes the phase-level plan or dependency structure. Otherwise, record the blocker in `TODO.md` and `./memory/claude_plan.md` without rewriting the project plan.
  5. Commit these changes and stop — the next invocation will pick up from there.

**Test/Fixture Failure Policy:**
- Any failing test or fixture is a real project issue, even if the failure already existed before the current task began.
- You must **not** ignore, dismiss, or work around failing tests/fixtures as pre-existing noise.
- The only exception is when the exact failure is already explicitly scheduled for repair in a later task or phase; a vague known-issue note is not enough.
- For every failing test/fixture that is not already explicitly scheduled, you must either fix it in the current task or add the minimum follow-up/prerequisite task(s) to `TODO.md` so the failure is scheduled before completion.
- Do not mark the current task `[DONE]` while leaving any newly observed unscheduled test/fixture failure unaddressed.

**No Workarounds, No Spec Deviations:**
- We do **not** tolerate workarounds, shims, fixture-only hacks, or “good enough for now” behavior when the implementation still does not match the spec.
- If you hit **anything** that does not work as the spec says — parser gaps, typecheck mismatches, lowering/codegen limitations, runtime bugs, stdlib gaps, incorrect diagnostics, or tests that only pass via workaround — you must treat that as a real project issue, not something to paper over.
- You must **not** continue by relying on a workaround unless the work of removing that workaround is itself explicitly tracked as a task in `TODO.md`.
- Blocking issues that are relevant to the current task take priority over forward progress. **Do not move on to the next planned task until the blocking issue is fixed, or until a new prerequisite task for fixing it has been inserted before the blocked task in `TODO.md`.**
- “Working around it” includes changing the intended representation, weakening the fixture, selecting a narrower test shape, introducing task-private special cases, or otherwise avoiding the broken path instead of repairing it.
- Instead, you must:
  1. Identify the spec mismatch precisely and determine whether it is a missing feature, a bug, or an incomplete implementation boundary.
  2. Create the corresponding prerequisite task in `TODO.md`, place it before any task that depends on it, and keep the dependency order explicit there.
  3. Ensure `TODO.md` reflects the new task ordering.
  4. Update the currently blocked task in `TODO.md` so it explicitly depends on the newly added fix task, if applicable.
  5. Update `PLAN.md` only if the mismatch changes the phase-level plan, stage dependency, or completion criteria.
  6. Commit these changes and stop.

**Missing or Incomplete Language Features:**
- If you encounter a task that requires a language feature or library that is not currently available, or any other implementation gap that prevents the spec-correct behavior, you must **not attempt to implement the task around that gap**. Instead:
  1. Identify the missing feature or incorrect behavior and research the details of its implementation or availability.
  2. Update `TODO.md` to reflect the dependency on the missing feature or bug fix, move the task to the appropriate position in the list, and add a dependency item from the current task to the newly added prerequisite task.
  3. Ensure `TODO.md` reflects the new ordering.
  4. Update `PLAN.md` only if the issue changes the phase-level plan or dependency structure.
  5. Commit these changes and stop.
- This applies even when the workaround seems local or convenient. If the intended shape is `enum Created(val start: () -> ...)` and that path is broken, you must fix that path or schedule it as a prerequisite; do not substitute a wrapper, alternate container, or special-case lowering just to keep moving.

**Code Organization & Quality:**
- **Quality:** Ensure that there is no warning during compilation and linting, e.g. `cargo clippy --all-targets -- -D warnings`.
- **Completeness:** Make sure that all features are implemented as planned. You must either **fully** complete the current task, or, if a concrete blocker makes that impossible, add the minimum prerequisite task(s) needed. Do **not** recursively decompose tasks for convenience.
- **Class-Wide Fixes Over Narrow Patches:** When fixing a defect or filling in a missing feature, do **not** artificially constrain yourself to the smallest local patch if the same root cause clearly affects a broader class of cases. Fix the whole identified class of problems, update the relevant tests/fixtures accordingly, and avoid knowingly leaving sibling cases broken just because only one instance was reported first.
- **Patch Application Discipline:** When editing code, do **not** generate one large patch spanning many files or unrelated hunks. Prefer multiple small, targeted patches applied incrementally, and re-read the affected file/section between patches when needed. If a large patch risks context mismatch or fails to apply, split it into smaller patches before continuing.
- **Modularity:** Break long source files into smaller, focused modules to improve readability and maintainability.
- **Tests:** If test files grow too large, split them into separate test modules or files.
- **Documentation:**
  - Add inline comments explaining the purpose and functionality of each function and module.
  - Maintain a root `README.md` with an overview, setup instructions, and usage examples.

**Completion & Release:**
1. If you find that all tasks in `TODO.md` are completed, perform a final review:
   - Verify that all features are implemented as planned.
   - Ensure all tests pass.
   - Confirm that the code is well‑organized and documented.
2. Commit any final adjustments with a message like "Complete project implementation."
3. Create a Git tag `endtag` to mark that all tasks are done.

**Important Reminders:**
- Always read `TODO.md` first for the actual task body and completion state.
- Complete exactly one task per invocation, then stop. A finished task must be visibly marked with `[DONE]` in its title before you move on.
- If a task's completion record has content but its title does not contain `[DONE]`, treat it as potentially unfinished and do not skip it.
- Use Git commits after every logical step (including plan updates or task decomposition) to maintain a clear history.
- Default to finishing the current task as written. Do **not** keep splitting tasks unless there is a concrete blocker that forces a prerequisite task.
- If you must add, remove, rename, reorder, or split tasks, update `TODO.md`.
- If you resume a task after a previous invocation failed unexpectedly and you now finish it, commit every currently uncommitted file together so the resumed task state is captured atomically.
- Never accept a workaround as “done”: every spec mismatch or workaround must be turned into an explicit task in `TODO.md` before proceeding.
- Update `PLAN.md` only for real phase/stage plan changes, not for routine per-task bookkeeping.
- If `PROMPT.md` is changed unexpectedly, include it in your commit as well, do not ignore or revert changes to it.
- Every single test case or fixture should run in under 1 minute. If you find any test case that gets stuck for a long time, it indicates a problem that must be addressed immediately.
- Run formatting and linting before expensive full validation: first `cargo fmt`, then `cargo clippy --all-targets -- -D warnings`, and only after those pass run the full test suite and full fixture suite. Otherwise, fixes from formatting or linting can force another expensive full test run.
- Full test-suite runs can take a long time. When running the complete Rust test suite, for example `cargo test --all --all-targets`, set a timeout of at least 30 minutes.
- Full fixture-suite runs can take a long time. When running the complete fixture suite, for example `python3 tools/run_fixtures.py`, set a timeout of at least 30 minutes.
- If no code has changed since the last successful full test-suite (or fixture-suite) run — for example, the current task only modifies documentation files such as `*.md`, `TODO.md`, `PLAN.md`, or comments-only edits that do not affect compiled output — you do not need to rerun that suite. Reuse the previous green result and note in the completion record that the suite was skipped because only documentation changed since the last full run.
