# Layout Fix Plan: One-Line Labels in `VStack` Demos

## Problem

In multiple demo binaries under `demos/`, `VStack` children default to `LayoutParams { height: Size::Fill, .. }`.
When several one-line text widgets (`Text`, `TextFn`, `Label`, `Divider`) are stacked with `Size::Fill`,
the `VStack` layout algorithm distributes remaining height across all `Fill` children. The views still *render*
as one line, but their **allocated slot height becomes > 1**, creating large vertical gaps and making labels
look like they “take too much space”.

Additionally, some demos already try to use `LayoutParams { height: Size::Content }` on nested `VStack`s.
Today that effectively collapses the nested stack to height `1` because `VStackView` does not report a
meaningful `desired_height()`.

## Goals

- In all `VStack`-related demos under `demos/`, ensure informational text labels occupy **no more than 1 row**.
- Make `Size::Content` usable for nested `VStack` sections by giving `VStackView` a real intrinsic height.
- Keep layout behavior deterministic and avoid changing unrelated APIs or defaults.

## Plan

### 1) Add intrinsic sizing for stack containers

Implement `desired_height()` (and `desired_width()` where sensible) for:

- `VStackView` (sum children’s intrinsic heights + spacing + padding + margins)
- `HStackView` (max children’s intrinsic heights + padding + margins)

Rules for intrinsic measurement:

- Ignore anchored children for flow sizing (treat as overlays).
- For `LayoutParams.height`:
  - `Fixed(h)`: contribute `max(h, child.min_height())`
  - `Content`: contribute `max(child.desired_height().unwrap_or(1), child.min_height())`
  - `Fill` / `Weight`: contribute `child.min_height()` (minimal footprint when asked for “content size”)
- Add margins and inter-item spacing between flow children.
- Add container padding.

This unlocks `LayoutParams { height: Size::Content }` for nested stacks and makes demos easier to lay out
without hard-coded fixed heights.

### 2) Update demo layouts to pin labels to one line

In each demo under `demos/` that uses `VStack`:

- For one-line text-ish children in a `VStack` (`Text`, `TextFn`, `Label`, `Divider`), switch from `.child(...)`
  to `.child_with_layout(..., LayoutParams { height: Size::Content, ..LayoutParams::default() })`.
  - Because these views report `desired_height() == Some(1)`, this guarantees the allocated slot height is `1`.
- Keep actual “expanding” elements (`Spacer`, main content panes/lists) as `Size::Fill` so they absorb leftover space.
- Where a nested `VStack` is intended to wrap its content (e.g. “Profile” / “Tuning” sections), use
  `LayoutParams { height: Size::Content, .. }` now that `VStackView::desired_height()` is implemented.

### 3) Add regression tests

Add unit tests (in `src/declarative/tests.rs`) to cover:

- `VStackView::desired_height()` returns the expected sum for a few children with:
  - `Size::Content` + desired heights
  - spacing + padding + margins
  - a `Size::Fill` child contributing only min height
- `HStackView::desired_height()` returns the expected max height for children.

These tests lock in the “`Size::Content` works for stacks” behavior without requiring PTY snapshots for demo binaries.

### 4) Validation

- Run `cargo fmt`
- Run `cargo test`
- (Optional) Run the affected demos manually:
  - `cargo run --bin demo-07-layout-management`
  - `cargo run --bin demo-06-data-binding`
  - `cargo run --bin demo-08-foreach-demo`
  - `cargo run --bin demo-09-custom-components`

## Expected Outcome

- Demo windows no longer show oversized vertical gaps for text labels inside `VStack`.
- Nested `VStack` sections laid out with `Size::Content` render at their natural height instead of collapsing to 1 row.
- No behavior change for stacks that are laid out with `Size::Fill`/`Size::Fixed` unless callers opt into `Size::Content`.

---

# Window Chrome Feature Plan: Fixed-Size Windows + Border Styles

## Problem

We need a few missing window-chrome features:

- **Fixed-size vs resizable windows**: the window manager already uses `Window.resizable` to gate resize handles, but
  the titlebar still shows **minimize/maximize buttons** even when the window cannot resize.
- **Border styles**: window decorations currently model `border` as a boolean, but we need three border modes:
  - `Normal` (current behavior)
  - `Thin` (always single-line border glyphs, even when focused)
  - `None/Borderless` (no border/titlebar chrome)

## Goals

- Add an explicit border style model to windows without changing default visuals (`Normal` stays the default).
- Treat `resizable = false` as “fixed size”: automatically hide minimize + maximize buttons and disable those actions.
- Keep borderless windows behaving like the existing `border = false` path (no chrome, view uses full rect, view-hosted scrollbars).
- Add a small set of unit tests to lock in behavior for thin borders and fixed-size button hiding.

## Plan

### 1) Introduce a `WindowBorderStyle` enum

- Add `WindowBorderStyle::{Normal, Thin, Borderless}` in `src/wm/window.rs`.
- Change `WindowDecorations.border: bool` to `WindowDecorations.border: WindowBorderStyle`.
- Update `Window::inner_rect()` and `Window::titlebar_rect()` to treat `Borderless` as “no chrome”.
- Re-export the new type from `src/wm/mod.rs` and `src/lib.rs`.

### 2) Update window manager drawing to respect border style

- When drawing window chrome in `src/wm/manager.rs`:
  - `Normal`: use `theme.border_set(is_focused)` (existing behavior).
  - `Thin`: use `theme.border_set(false)` (single-line glyphs), but keep focused border *style*.
  - `Borderless`: skip border/titlebar rendering.
- Keep scrollbar host selection consistent:
  - With border (normal/thin): `ScrollbarHost::Window`
  - Borderless: `ScrollbarHost::View`

### 3) Hide min/max when `resizable == false` (“fixed size”)

- Compute “effective titlebar buttons” from `(decorations.buttons, window.resizable)`:
  - If `!window.resizable.get()`: force `{ minimize: false, maximize: false }`
- Use the effective buttons for:
  - `draw_titlebar(...)`
  - `hit_test_buttons(...)`
  - mouse handlers for min/max clicks
  - keyboard shortcuts (`m` minimize, `x` maximize)

### 4) Tests + validation

- Add unit tests in `src/wm/manager.rs` to verify:
  - Thin border draws single-line glyphs even when focused.
  - Fixed-size windows do not respond to minimize/maximize shortcuts (and don’t render those buttons).
- Run `cargo fmt` and `cargo test`.
