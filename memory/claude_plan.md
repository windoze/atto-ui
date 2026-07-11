## Execution Plan

I will work from `TODO.md` as the source of truth and complete exactly the first task whose heading is not prefixed with `[DONE]`.

1. Read `TODO.md` to identify the first incomplete task and its validation requirements.
2. Check the latest commit message only for unfinished work directly relevant to that selected task.
3. Inspect only the files needed to implement the selected task.
4. Implement the task without changing unrelated behavior.
5. Run formatting, clippy with warnings denied, and the required tests unless the task only changes documentation and a previous green full run can be reused.
6. Update `TODO.md` by prefixing the completed task heading with `[DONE]` and filling its completion record.
7. Commit all changes related to this invocation, then stop without starting the next task.

Progress:
- Created this execution plan before reading or modifying project files.
- Identified first incomplete task: `M2.1 死窗口回收`.
- Current task plan: inspect the terminal shell/window integration, implement process-exit detection that leaves a visible restart prompt or closes according to existing policy, cover it with targeted PTY/unit tests, then run required validation and mark only M2.1 done.
- Implemented shell-level terminal session tracking in the terminal viewer and PTY fixture: exited child processes now release capture, show the configured exit prompt, and restart the focused dead terminal when plain `R` is pressed.
- Added a PTY regression that launches a child shell, verifies the exit prompt/status, presses `R`, and observes the restart counter.
- First focused PTY run found the new fixture process status line was clipped by the status window; expanded the fixture status window to fit all four status lines.
- Validation completed successfully: formatting, clippy with warnings denied, the focused PTY regression, and the full workspace test suite all passed.
- Updated `TODO.md` to mark only `M2.1 死窗口回收` as `[DONE]` with its completion record.
