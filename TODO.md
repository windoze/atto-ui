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

- [x] Focused window should have double-line border
- [x] Window should be resizable by mouse dragging the bottom right corner
- [x] Window closing hooks, the user should be able to register a callback that is called when the user tries to close the window, e.g., to pop up a "Are you sure?" dialog.
- [x] Dropdown menus should also have drop shadows
- [x] Menu item in the menu bar should also have an optional keyboard shortcut, e.g. "Alt+F" to open the "File" menu.
- [x] Menu items in drop down menus should have an optional keyboard shortcuts, e.g., when the "File" menu is open, pressing "N" should trigger the "New File" action.

## Mouse Support

Mouse support is implemented in layered slices so routing stays deterministic and testable.

### Coordinate system

- Use Crossterm mouse events (`MouseEvent { kind, column, row, modifiers }`) as the raw input.
- Treat `(column, row)` as **absolute screen-cell coordinates** in the same space as `Rect { x, y }`.
- Hit-testing compares absolute coordinates against the last-drawn `Rect`s for desktop chrome,
  windows, and widgets.

### Routing (desktop → window manager → focused view)

- Desktop chrome gets first refusal for mouse-down:
  - menu bar: clicks activate/switch menus
  - status bar: clicks are consumed (no focus changes)
- While a menu is active, it acts as an **event sink**:
  - clicking outside closes the menu
  - clicks do not fall through to underlying windows/views
- The window manager then hit-tests window chrome:
  - title bar drag moves windows
  - bottom-right corner drag resizes windows
  - buttons (min/max/close) consume the click
  - body clicks focus/raise a window first, then dispatch to the focused view

### Optional gesture layer (phase 2)

- Keep derived gestures (double-click, hover enter/leave, drag selection, wheel scrolling) separate
  from raw terminal mouse events so timing-sensitive behavior can be disabled in tests.

### Testing

- Prefer PTY tests with deterministic input scripts:
  - fixed terminal sizes
  - SGR mouse sequences for click and drag
