# TODO

## Mis-designs

- [x] Layered event handling, e.g., when a modal window is open, only that window should receive input events. Events may bubble up to parent views/windows if not handled. A typical flow is:
    1. Focused view receives event first
    2. If not handled, focused window receives event
    3. If not handled, application receives event
    4. Some events sinks may choose to stop event propagation, e.g., modal windows.

## Bugs

- [x] Drop down menu should be above other content, now it is below window
- [x] Modal window (include dropdown menus) should grab the input so global shortcuts do not trigger actions when modal is open
- [x] Drop shadow missing the bottom right corner
- [x] View/Window should have a solid background by default, now it is transparent, that is, the content behind it is visible if foreground has no text.

## Missing Features and Improvements

- [ ] Focused window should have double-line border
- [ ] Window should be resizable by mouse dragging the bottom right corner
- [ ] Window closing hooks, the user should be able to register a callback that is called when the user tries to close the window, e.g., to pop up a "Are you sure?" dialog.
- [ ] Dropdown menus should also have drop shadows
- [ ] Menu item in the menu bar should also have an optional keyboard shortcut, e.g. "Alt+F" to open the "File" menu.
- [ ] Menu items in drop down menus should have an optional keyboard shortcuts, e.g., when the "File" menu is open, pressing "N" should trigger the "New File" action.

## Mouse Support

Mouse support is a relatively big feature and needs a clear split between:
1) raw terminal mouse input, 2) routing/hit-testing, and 3) higher-level gestures.

### 1) Raw terminal input (Crossterm-level)

- Use Crossterm's mouse events (`MouseEvent { kind, column, row, modifiers }`) as the raw input.
- Coordinates should be defined as **screen-cell positions** (0-based, like Crossterm), and interpreted relative to the current `Rect` for hit-testing.
- Treat the event stream as **best-effort**:
  - Some terminals do not report all mouse kinds (e.g. motion events) unless tracking is enabled.
  - Some terminals/platforms do not reliably report all modifier combinations for all mouse kinds.

### 2) Hit-testing + routing (Chatty-level)

- Add a central router that maps `(x, y)` to a target:
  - Desktop chrome (menu bar, status bar)
  - Window chrome (borders/titlebar/buttons/resize handle)
  - Window body (the `View` inside the window's inner rect)
- Routing should follow the same layered rules as keyboard input:
  - If a modal is active, it is an **event sink** (events do not bubble to global shortcuts).
  - Otherwise, route to the best target and bubble only if the target ignores the event.
- Mouse events often need **pre-routing focus policy**:
  - On mouse-down, the window manager may need to hit-test and change focus/z-order first.
  - After focus is updated, the event can be dispatched to the now-focused view/control.

### 3) Derived gestures (optional layer on top of raw events)

Raw terminals events are usually just `Down/Up/Drag/Moved/Scroll*`. Concepts like "click",
"double-click", "hover enter/leave", or "drop" should be treated as **derived gestures**:

- Implement a small gesture recognizer that consumes raw events and emits higher-level events:
  - `Click` (Down+Up on same target)
  - `DoubleClick` (two clicks within a configurable timeout)
  - `DragStart/DragMove/DragEnd` (with optional threshold)
  - `HoverChanged { entered/exited }` (only if motion events are available/enabled)
- Keep this layer separate so it can be disabled or made deterministic for tests.

### 4) Focus policy

- Mouse clicks should be able to trigger focus changes.
- Clicking a focusable control should support "click-to-activate" in one interaction:
  - focus changes first, then the same click can trigger the control action (no second click).
- Focus gained/lost notifications should not require mouse coordinates; instead include an optional
  **cause** (mouse / keyboard / programmatic) and keep pointer state separately.

### 5) Pointer/hover state

- Avoid "views query the global mouse position" as a primary API.
- Prefer passing pointer state through context (or explicit enter/leave events) so hover effects are
  derived from routing, not from ad-hoc global queries.

### 6) Testing expectations

- Prefer PTY tests that use deterministic scripts:
  - Click and drag using SGR mouse encoding
  - Avoid depending on motion/hover/double-click unless explicitly enabled and deterministic
