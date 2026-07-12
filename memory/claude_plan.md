# Execution plan

I will follow `TODO.md` as the source of truth and complete only the first task whose heading is not prefixed with `[DONE]`.

## Current task

First incomplete task: `M7.4 配置界面`.

Task requirement: add a visual settings window for terminal configuration. It should reuse the declarative layout components (`VStack`/`HStack`/`Grid`) and existing widgets (`TextBox`/`Checkbox`/`RadioGroup`/`ListBox`) to edit grouped configuration values, support immediate preview/apply and save, and be opened from a menu entry.

Latest commit check: the latest commit is `[M7.3] Add terminal configuration model`; it does not explicitly mention unfinished work that must preempt M7.4.

## Steps

1. Inspect the terminal configuration API added by M7.3, the terminal viewer/window fixture menu code, existing widget APIs, and existing settings/dialog patterns.
2. Design a small settings view model that can edit the M7.3 `TerminalConfig` fields that are safe to expose in this task without changing behavior outside the settings flow.
3. Add a reusable terminal settings component/window that uses declarative containers and existing widgets for grouped controls.
4. Wire the settings window into the terminal app menu entry and support Apply/Save actions. Immediate runtime behavior can update the active in-memory config state now; deeper component-wide configuration plumbing remains M7.5.
5. Add focused tests for the settings model/component behavior and any PTY/menu coverage that belongs to this task.
6. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and relevant tests/full workspace tests as required.
7. Mark M7.4 `[DONE]` in `TODO.md`, record implementation and validation, commit all changes, and stop.

## Progress

- Identified `M7.4 配置界面` as the first incomplete task.
- Confirmed latest commit is M7.3 and contains no explicit unfinished blocker for M7.4.
- Added a reusable terminal settings module with a declarative settings view, draft/config conversion, Apply/Save/Reset actions, preview/status text, default config path helpers, and unit tests.
- Wired the settings window into the interactive terminal viewer and deterministic PTY fixture File menus.
- Added PTY coverage for opening the settings window from File -> Settings and saving a config file.
- Validation completed: formatting, focused settings unit tests, settings PTY test, workspace clippy with warnings denied, and full workspace tests all passed.
- Updated `TODO.md` to mark M7.4 as `[DONE]` with a completion record.
