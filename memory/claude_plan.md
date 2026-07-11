# Execution Plan

I will follow `TODO.md` as the authoritative task list and complete only the first task whose heading is not prefixed with `[DONE]`.

Current task: `M4.2 鼠标本地框选` — implement local terminal mouse selection so `Shift+drag` selects locally when mouse reporting is enabled, plain drag is forwarded to the subprocess, and drag selects locally when mouse reporting is disabled. Also fix wasted-click recapture around `capture_on_click`.

1. Read `TODO.md` to identify the first incomplete task and its requirements.
2. Check the latest commit message only for unfinished work directly relevant to that task.
3. Inspect `atto-ui-terminal` terminal mouse handling, selection state, PTY fixtures, and relevant tests.
4. Implement local mouse selection and mouse-reporting forwarding rules without changing unrelated terminal behavior. **Done:** terminal component mouse handling now starts/updates/finishes local selection for plain drag without mouse reporting and for Shift+drag with mouse reporting; plain mouse-reporting drags still forward to the subprocess, and capture-on-click recapture continues processing the original click.
5. Add or update targeted tests for selection start/update/end, mouse-reporting forwarding, and capture-on-click behavior. **Done:** `input_encoding` now covers plain local drag selection, mouse-reporting plain forwarding, mouse-reporting Shift local selection, and recapture click forwarding.
6. Run formatting, clippy with warnings denied, and the relevant/full test suite as required. **Done:** `cargo fmt --all`, `cargo test -p atto-ui-terminal --test input_encoding -- --nocapture`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --all-targets` passed.
7. Update `TODO.md` by prefixing the completed task title with `[DONE]` and filling in its completion record. **Done:** `M4.2` is marked complete with implementation and validation notes.
8. Commit all changes for this task with a descriptive message and stop. **Next:** review final status and create the task commit.
