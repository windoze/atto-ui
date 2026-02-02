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

