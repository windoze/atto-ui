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
- Identified first incomplete task: `M2.2 标题联动`.
- Latest commit is `[M2.1] Add terminal process exit recovery`; it does not describe unfinished work that changes M2.2 ordering.
- Current task plan: inspect terminal title callback APIs, shell/window title ownership, Windows menu list refresh paths, and existing PTY fixtures; implement title propagation from terminal OSC 0/2 events into `Window.title`; ensure the Windows menu list uses updated titles; add targeted PTY coverage; run required formatting, clippy, and tests; mark only M2.2 done and commit.
- Inspected the terminal callback/handle APIs, viewer shell session tracking, Desktop `set_title`, Window title storage, menu mutation APIs, and the terminal window PTY fixture. Implementation will poll `TerminalHandle::window_title()` from the shell tick/action loop and update `Window.title` on the UI thread before refreshing the Windows menu.
- Implemented UI-thread polling of terminal OSC titles in `terminal_viewer` and the snapshot terminal window fixture, reset default titles on restart, refreshed the Windows menu from current `Window.title`, and added a PTY regression for titlebar/menu propagation.
- Validation completed successfully: formatting, format check, clippy with warnings denied, the focused OSC title PTY regression, and the full workspace test suite all passed.
- Updated `TODO.md` to mark only `M2.2 标题联动` as `[DONE]` with its completion record.
