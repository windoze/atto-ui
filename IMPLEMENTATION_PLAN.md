# Chatty — Implementation Plan

This document breaks down development into milestones with concrete deliverables and test/validation steps.

## Principles

- **Incremental, testable slices:** each milestone must compile and have automated validation.
- **Deterministic rendering tests:** prefer PTY-backed, screen-buffer snapshot tests for end-to-end behavior.
- **Modular architecture:** low-level primitives (views/widgets/themes) build up to a window manager and desktop UI.
- **Turbo Vision-inspired UX:** keyboard-driven window/menu operations with mouse support layered on.

## Repository Layout (target)

- `src/` — `chatty` library crate
  - `app/` — run-loop helpers and top-level desktop integration
  - `wm/` — window manager (z-order, focus, move/resize, modal)
  - `views/` — view traits + common built-in views (optional)
  - `widgets/` — reusable widgets (button, textbox, checkbox, …)
  - `theme/` — theming/styling
  - `text/` — Unicode-aware text editing helpers (IME/paste-friendly)
- `crates/chatty-test-host/` — PTY test host library for integration tests
- `tests/` — end-to-end tests using PTY host + a test binary
- `examples/` — demo application showcasing capabilities

## Test Strategy

### Unit-level

- Pure logic tests for:
  - window manager state transitions (focus, z-order changes)
  - geometry clamping (move/resize within desktop bounds)
  - Unicode text buffer editing (insert/delete, cursor movement by grapheme)

### End-to-end (required)

Create a **test-host app** that:

1. Allocates a **PTY** with a fixed terminal size.
2. Runs a TUI binary connected to that PTY.
3. Captures output and parses it into a **screen buffer**.
4. Simulates user input:
   - keyboard sequences (arrows, function keys, modifiers where possible)
   - mouse sequences (xterm SGR mouse)
   - bracketed paste sequences to simulate IME/paste input

Tests assert against:

- **presence checks** (e.g., window titles visible),
- **state-driven snapshots** (stable expected screen text after scripted inputs),
- **behavior checks** (e.g., focused window changes after click).

### Manual validation

- `cargo run --example demo`
- Verify:
  - moving/resizing via keyboard + mouse drag
  - z-order changes on focus
  - modal windows block interaction
  - paste inserts Unicode text correctly

## Milestones

### M0 — Bootstrap (crate + tooling)

**Deliverables**

- Cargo crate initialized
- Basic CI-friendly commands documented (build/test)

**Validation**

- `cargo build`
- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - Cargo workspace created for `chatty` + `crates/chatty-test-host`
  - Smoke builds/tests: `cargo build`, `cargo test`

---

### M1 — Core API + Theme + Text

**Deliverables**

- `View` trait (render + event handling)
- `Theme` struct with styles for desktop/window/widget primitives
- Unicode-aware `TextBuffer` for textbox editing and paste/IME-like input

**Tests**

- Unit tests for `TextBuffer` (grapheme cursor movement; delete/backspace behavior)

**Validation**

- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - `src/view.rs`: `View` trait + `ViewAction`
  - `src/theme/mod.rs`: `Theme` (dark/light)
  - `src/text/buffer.rs`: grapheme-aware `TextBuffer` + unit tests

---

### M2 — Window System (z-order, focus, move/resize)

**Deliverables**

- `Window` model:
  - kinds: normal/floating/modal/tooltip
  - states: normal/minimized/maximized/closed
- `WindowManager`:
  - create/destroy windows
  - focus + z-order maintenance
  - keyboard-driven move/resize
  - mouse focus + titlebar dragging

**Tests**

- Unit tests for window manager state transitions (no rendering)

**Validation**

- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - `src/wm/window.rs`: window model (kind/state/decorations)
  - `src/wm/manager.rs`: z-order, focus, move/resize, mouse drag/titlebar buttons + unit tests

---

### M3 — Rendering + Desktop Chrome (decorations, menu, status)

**Deliverables**

- Window decorations:
  - border + title bar
  - control buttons (min/max/close)
  - optional drop shadow
- Desktop chrome:
  - menubar (F10 activation, nested menus)
  - status bar (dynamic hints + state)

**Tests**

- PTY-backed snapshot tests for:
  - correct window layering / z-order
  - title bars and borders
  - menu activation and navigation

**Validation**

- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - `src/app/desktop.rs`: desktop layout + mode switching (normal/menu/window)
  - `src/app/menu.rs`: nested menus (F10) + command dispatch
  - `src/app/status.rs`: status bar rendering

---

### M4 — Widgets (button, label, textbox, checkbox, radio, list, table)

**Deliverables**

- Reusable widgets with basic focus + keyboard interaction
- A simple container (`Form`) to manage focus traversal (Tab/Shift-Tab)

**Tests**

- PTY-backed snapshot tests verifying:
  - checkbox toggles
  - textbox accepts paste (including Unicode)
  - list selection moves with arrows

**Validation**

- `cargo test`

**Progress**

- [x] Complete
- Notes:
  - `src/widgets/`: `Label`, `TextBox` (paste + Unicode), `Checkbox`, `RadioGroup`, `ListBox`, `TableView`, `Button`
  - `src/widgets/primitives.rs`: `Form` for focus traversal (Tab/Shift-Tab)

---

### M5 — Test Host + Demo App

**Deliverables**

- `crates/chatty-test-host`:
  - spawn a binary in a PTY
  - read output + parse to screen buffer
  - send keyboard/mouse/paste sequences
- `examples/demo.rs`:
  - multi-window desktop
  - modal dialog + tooltip
  - menubar + status bar
  - theming toggle
  - widgets showcase

**Tests**

- At least 2 PTY end-to-end tests:
  - window movement/z-order and close
  - paste event into textbox and Unicode rendering

**Validation**

- `cargo test`
- `cargo run --example demo` (manual)

**Progress**

- [x] Complete
- Notes:
  - `crates/chatty-test-host`: PTY runner + vt100 screen parsing + keyboard/mouse/paste helpers
  - `tests/pty_desktop.rs`: PTY integration tests (paste, window move, mouse focus)
  - `src/bin/snapshot_app.rs`: deterministic test binary
  - `examples/demo.rs`: interactive demo showcasing features

---

## Mouse Support — User Stories

These user stories describe the next slices for mouse support. They intentionally separate
**raw terminal mouse input**, **hit-testing/routing**, and optional **gesture recognition**
(click/double-click/drag/hover), so the system stays deterministic and testable.

### Completed

- [x] Click a window to focus/raise it
  - Acceptance:
    - Clicking anywhere inside a focusable window brings it to front and focuses it.
    - When a modal is active, clicks outside the modal do not change focus.
  - Tests:
    - PTY: `tests/pty_desktop.rs` focus changes after click.

- [x] Drag a window by its title bar (move)
  - Acceptance:
    - Left-dragging a window title bar moves the window within desktop bounds.
    - Modal windows are not movable; maximized windows do not move.
  - Validation:
    - Manual: `cargo run --example demo` and drag the title bar.

- [x] Click window chrome buttons (min/max/close)
  - Acceptance:
    - Clicking `×` closes a closable window.
    - Clicking `□` toggles maximize.
    - Clicking `−` minimizes and updates focus to the next focusable window.
  - Validation:
    - Manual: `cargo run --example demo`.

- [x] Resize a window by dragging the bottom-right corner
  - Acceptance:
    - Left-dragging the bottom-right corner resizes the window within desktop bounds.
    - Modal windows are not resizable; maximized windows do not resize.
  - Tests:
    - Unit: `src/wm/manager.rs` resize handle drag.

- [x] Desktop-level mouse routing (chrome vs windows)
  - As a user, I can click desktop chrome (menu bar, status bar) without accidentally interacting
    with windows behind it.
  - Acceptance:
    - Hit-testing distinguishes desktop chrome vs window regions.
    - Mouse events are routed to the best target and only bubble when ignored.
  - Tests:
    - PTY: `tests/pty_mouse_support.rs` menu clicks and click-outside close.

- [x] Click menu bar items to open drop-down menus
  - As a user, I can open a menu by clicking its title (e.g. `File`) without using F10.
  - Acceptance:
    - Clicking a menu title activates the menu bar and opens the corresponding drop-down.
    - Clicking a different title switches the open menu.
    - Clicking outside the menu closes it.
  - Tests:
    - PTY: `tests/pty_mouse_support.rs` menu open/switch/close.

- [x] Click drop-down menu items to trigger actions and open submenus
  - As a user, I can execute menu commands with the mouse.
  - Acceptance:
    - Clicking an enabled leaf item emits its command and closes the menu.
    - Clicking an item with a submenu opens that submenu.
    - Disabled items do not trigger commands.
  - Tests:
    - PTY: `tests/pty_mouse_support.rs` About opens a modal; Theme → Light toggles theme.

- [x] Window focus changes on click are consistent
  - As a user, when I click in a different window, it becomes focused and raised.
  - Acceptance:
    - Clicking a non-focused window focuses it before dispatching the click to its view.
    - When a modal is open, clicking other windows has no effect.
  - Tests:
    - PTY: `tests/pty_desktop.rs` click swaps focus.

- [x] Widget mouse interactions (click-to-activate)
  - As a user, I can click a focusable widget to focus it and activate it in one interaction.
  - Acceptance:
    - The first click both focuses the control and triggers its action when appropriate.
    - Controls can ignore mouse events they don't care about.
  - Tests:
    - PTY: `tests/pty_mouse_support.rs` checkbox toggles on click.

- [x] TextBox: click to set cursor position
  - As a user, clicking inside a textbox moves the caret to that position.
  - Acceptance:
    - The caret position uses grapheme-aware indexing (Unicode-safe).
    - Clicking past the end of the text places the caret at end.
  - Tests:
    - PTY: `tests/pty_mouse_support.rs` click sets caret, then typing inserts at that position.

### Planned

- [ ] TextBox: click-and-drag selection (optional, phase 2)
  - As a user, I can select text by dragging the mouse across a textbox.
  - Acceptance:
    - Drag selection highlights a range of graphemes.
    - Selection updates as the pointer moves while pressed.
  - Tests:
    - PTY: drag selects; typing replaces selection (or clears selection, depending on chosen UX).

- [ ] Scroll wheel support for scrollable widgets (optional, phase 2)
  - As a user, I can scroll lists/tables with the mouse wheel when content overflows.
  - Acceptance:
    - `ScrollUp/ScrollDown` updates the scroll offset with bounds clamping.
  - Tests:
    - PTY: send wheel events and assert visible rows change.

- [ ] Gesture recognition layer (click/double-click/hover) kept separate
  - As a developer, I can build higher-level gestures without polluting the core event API.
  - Acceptance:
    - Raw events remain `Down/Up/Drag/Moved/Scroll*` (terminal-native).
    - `Click/DoubleClick/HoverChanged` are emitted by an optional recognizer with configurable timing.
  - Tests:
    - Unit: recognizer emits click/double-click given synthetic timestamps.

## Versioning Targets (suggested)

- `0.1.0` — MVP desktop + windows + a few widgets + PTY tests
- `0.2.0` — richer widget set + improved mouse handling + better menu system
- `0.3.0` — layout containers, docking, more Turbo Vision-like dialogs

---

## M6 — View Hierarchy & Layout Management

This milestone introduces a flexible view hierarchy system with layout managers, enabling developers to build complex nested UI structures with automatic positioning and sizing of child views.

### Prerequisites

- Current `View` trait needs extension to support child views
- Layout calculations must account for window decorations and desktop chrome
- Event routing must traverse the view hierarchy correctly

### User Stories

#### US-6.1: View Hierarchy Foundation

**As a** developer
**I want to** create views that can contain child views
**So that** I can build complex nested UI structures

**Acceptance Criteria:**

- [x] `View` trait is extended to support child view management
- [x] Parent views can add, remove, and query child views
- [x] Child views maintain a reference to their parent (or parent area)
- [x] View hierarchy supports arbitrary nesting depth
- [x] Each view maintains its bounds (position and size) relative to its parent

**Tests:**

- Unit: create a parent view with multiple children; verify child count and retrieval
- Unit: add/remove children dynamically; verify collection updates
- Unit: nested views (grandparent → parent → child) maintain correct hierarchy

**Notes:**

- Consider using `Vec<Box<dyn View>>` for storing children
- May need `ViewId` or similar identifier for addressing specific child views
- Parent views should own their children for lifetime management
- Child bounds should be `Rect` relative to parent's content area (not absolute)

---

#### US-6.2: Event Routing Through View Hierarchy

**As a** developer
**I want** events to be automatically routed to the appropriate child view
**So that** child views can handle user input without manual dispatching

**Acceptance Criteria:**

- [x] Mouse events are routed to the deepest child view at the event coordinates
- [x] Keyboard events are routed to the focused child view (if any)
- [x] Events bubble up to parent if child returns `EventOutcome::Ignored`
- [x] Parent views can intercept events before children receive them (capture phase)
- [x] Event coordinates are translated to child-relative coordinates automatically

**Tests:**

- Unit: click at coordinates within a child's bounds → child receives the event
- Unit: child ignores event → parent's `handle_event` is called
- PTY: click a button inside a nested container → button action fires

**Notes:**

- May need to extend `ViewEventResult` to support event bubbling control
- Hit-testing should respect z-order for overlapping children
- Consider adding a focus chain for keyboard navigation through child views

---

#### US-6.3: Vertical Layout Container (VBox)

**As a** developer
**I want** a vertical layout container that arranges children from top to bottom
**So that** I can create forms, lists, and column-based layouts

**Acceptance Criteria:**

- [x] `VBox` arranges children vertically, each taking full width of the container
- [x] Child height can be specified as:
  - Fixed pixels (e.g., `10` rows)
  - Proportional weight (e.g., `1.0` shares remaining space equally)
  - Content-based (child reports its desired height)
- [x] Children are laid out in the order they were added
- [x] Layout respects container bounds and does not overflow
- [x] If total child height exceeds container, layout is clamped (overflow handled separately)

**Tests:**

- Unit: VBox with 3 children (fixed heights 5, 10, 5) in 20-row container → correct bounds
- Unit: VBox with weighted children (1.0, 2.0) splits space 1:2
- Unit: VBox in 10-row container with children totaling 15 rows → layout clamps to bounds
- PTY: render a form with label + textbox + button stacked vertically

**Notes:**

- Layout algorithm runs during `draw()` or as a separate layout pass before rendering
- Consider adding `spacing` parameter for gaps between children
- May need a `LayoutParams` struct to specify child sizing constraints

---

#### US-6.4: Horizontal Layout Container (HBox)

**As a** developer
**I want** a horizontal layout container that arranges children from left to right
**So that** I can create toolbars, button groups, and row-based layouts

**Acceptance Criteria:**

- [x] `HBox` arranges children horizontally, each taking full height of the container
- [x] Child width can be specified as:
  - Fixed columns (e.g., `20` columns)
  - Proportional weight (e.g., `1.0` shares remaining space equally)
  - Content-based (child reports its desired width)
- [x] Children are laid out in the order they were added (left to right)
- [x] Layout respects container bounds and does not overflow
- [x] If total child width exceeds container, layout is clamped (overflow handled separately)

**Tests:**

- Unit: HBox with 3 children (fixed widths 10, 20, 10) in 40-column container → correct bounds
- Unit: HBox with weighted children (1.0, 3.0) splits space 1:3
- Unit: HBox in narrow container → layout clamps to bounds
- PTY: render a toolbar with multiple buttons arranged horizontally

**Notes:**

- Layout logic mirrors VBox but operates on the horizontal axis
- Consider Unicode width calculations for content-based sizing

---

#### US-6.5: Grid Layout Container

**As a** developer
**I want** a grid layout container that arranges children in rows and columns
**So that** I can create structured forms and data grids

**Acceptance Criteria:**

- [x] `Grid` arranges children in a grid with a specified column count
- [x] Children fill grid cells left-to-right, top-to-bottom
- [x] All cells in a row have equal height (row height = tallest child in that row)
- [x] All cells in a column have equal width (column width = container width ÷ column count)
- [x] Grid handles partial rows (last row may have fewer cells than columns)
- [x] Layout respects container bounds

**Tests:**

- Unit: Grid with 2 columns, 5 children → 3 rows (2+2+1 cells)
- Unit: Grid with 3 columns in 60-column container → each cell is 20 columns wide
- Unit: Grid with varying child heights → row height matches tallest child
- PTY: render a grid of checkboxes in 2 columns

**Notes:**

- Consider adding row-span and column-span support (future enhancement)
- May need to measure child preferred sizes before layout
- Consider adding `row_gap` and `column_gap` parameters

---

#### US-6.6: Padding and Margin Support

**As a** developer
**I want** to specify padding and margins for layout containers
**So that** I can control spacing inside and around containers

**Acceptance Criteria:**

- [x] Containers support `padding` (inner spacing) that reduces the content area
- [x] Views support `margin` (outer spacing) that reserves space around the view
- [x] Padding/margin can be specified for all sides or individually (top, right, bottom, left)
- [x] Layout calculations automatically account for padding and margins
- [x] Padding affects child layout area; margin affects view's allocated space from parent

**Tests:**

- Unit: VBox with padding=2 → children laid out within padded area
- Unit: VBox with child margins → spacing between children
- Unit: nested containers with padding and margins → correct space calculations
- PTY: render a container with visible padding (border shows padding area)

**Notes:**

- Use a `Spacing` or `EdgeInsets` struct: `{ top, right, bottom, left }`
- Default padding/margin should be zero
- Consider whether padding is inside or outside borders for windows

---

#### US-6.7: Alignment Options in Layouts

**As a** developer
**I want** to align child views within their allocated space
**So that** I can position views at start/center/end of their layout cell

**Acceptance Criteria:**

- [x] Layout containers support horizontal alignment: `Start`, `Center`, `End`
- [x] Layout containers support vertical alignment: `Start`, `Center`, `End`
- [x] When a child's natural size is smaller than its allocated space, alignment controls positioning
- [x] Alignment applies independently on both axes
- [x] Default alignment is `Start` for both axes

**Tests:**

- Unit: VBox with center-aligned children → children centered horizontally within full width
- Unit: HBox with end-aligned children → children aligned to bottom of container
- Unit: Grid with mixed alignments → each cell respects its alignment setting
- PTY: render a button centered in a wide container

**Notes:**

- Alignment enum: `Align::Start | Center | End`
- May need per-child alignment overrides in addition to container-wide defaults
- Consider adding `Stretch` option to make child fill allocated space

---

#### US-6.8: Anchor-Based Positioning

**As a** developer
**I want** to anchor views to specific edges or corners of their parent
**So that** I can precisely position fixed-size views (e.g., close button at top-right)

**Acceptance Criteria:**

- [x] Views can be anchored to parent edges: `Top`, `Bottom`, `Left`, `Right`
- [x] Views can be anchored to parent corners: `TopLeft`, `TopRight`, `BottomLeft`, `BottomRight`
- [x] Anchored views maintain their position relative to the anchor point when parent resizes
- [x] Anchoring can be combined with offset (e.g., "10 pixels from top-right corner")
- [x] Anchored views do not participate in normal layout flow

**Tests:**

- Unit: view anchored to `TopRight` → positioned at parent's top-right corner
- Unit: view anchored to `Bottom` with offset → positioned at parent bottom minus offset
- Unit: parent resizes → anchored view maintains position relative to anchor
- PTY: render a container with a close button anchored to top-right

**Notes:**

- Anchor enum: `Anchor::TopLeft | TopRight | BottomLeft | BottomRight | Top | Bottom | Left | Right | Center`
- Anchored views need explicit size (width/height)
- Consider whether anchored views are part of child collection or separate

---

## M7 — Viewport & Scrolling

This milestone adds scrolling support to views, allowing content larger than the visible area to be navigated via keyboard, mouse wheel, or scrollbar dragging.

### Prerequisites

- View hierarchy (M6) must be complete
- Layout system must calculate content size vs. viewport size

### User Stories

#### US-7.1: Viewport and Content Offset

**As a** developer
**I want** views to maintain a viewport (visible area) and content offset
**So that** I can render content larger than the view's bounds

**Acceptance Criteria:**

- [x] Views can track a content size (total size of all content)
- [x] Views maintain a scroll offset (x, y) representing the top-left of the viewport
- [x] Content rendering is clipped to the viewport bounds
- [x] Scroll offset is clamped to valid range: `[0, content_size - viewport_size]`
- [x] Views with content smaller than viewport have zero scroll offset

**Tests:**

- Unit: view with content size 100×50, viewport 20×10 → max scroll offset is (80, 40)
- Unit: set scroll offset beyond bounds → clamped to valid range
- Unit: content size smaller than viewport → scroll offset stays at (0, 0)

**Notes:**

- Content size may be calculated from child views or set explicitly
- Scroll offset is typically (0, 0) for top-left origin
- Consider separate horizontal and vertical scrolling enable flags

---

#### US-7.2: Keyboard Scrolling

**As a** user
**I want** to scroll content using arrow keys and Page Up/Down
**So that** I can navigate through content using the keyboard

**Acceptance Criteria:**

- [x] Arrow keys scroll content by one line/column
- [x] Page Up/Down scroll content by one viewport height
- [x] Home/End scroll to top/bottom of content (vertical) or left/right (horizontal)
- [x] Scrolling respects content bounds (does not scroll beyond content)
- [x] Scrolling updates immediately and re-renders the view

**Tests:**

- PTY: arrow down in scrollable view → content scrolls by one line
- PTY: Page Down → content scrolls by viewport height
- PTY: Home/End → content scrolls to top/bottom
- PTY: attempt to scroll beyond content → no change

**Notes:**

- Scroll step size should be configurable (default 1 line/column for arrows)
- Consider whether Ctrl+Home/End should scroll to document start/end
- May need to distinguish between view-level scrolling and widget-internal scrolling (e.g., textbox cursor)

---

#### US-7.3: Mouse Wheel Scrolling

**As a** user
**I want** to scroll content using the mouse wheel
**So that** I can navigate quickly with the mouse

**Acceptance Criteria:**

- [x] Mouse wheel up/down scrolls content vertically by a configurable step (default 3 lines)
- [x] Mouse wheel scrolling respects content bounds
- [x] Wheel events are routed to the view under the mouse cursor
- [x] If a view does not support scrolling, wheel events bubble to parent

**Tests:**

- PTY: send `ScrollUp` event → content scrolls up by configured step
- PTY: send `ScrollDown` event → content scrolls down by configured step
- PTY: wheel scroll at content boundary → no overflow

**Notes:**

- Wheel scroll step size should be configurable (default 3 lines)
- Consider smooth scrolling vs. stepped scrolling
- Horizontal wheel scrolling (Shift+Wheel or trackpad) should also be supported

---

#### US-7.4: Scroll Clipping vs. Scrollable Container

**As a** developer
**I want** to choose whether content is clipped or scrollable
**So that** I can control overflow behavior per view

**Acceptance Criteria:**

- [x] Views have a `scrollable` flag (default `false`)
- [x] Non-scrollable views clip child content to viewport bounds (overflow is hidden)
- [x] Scrollable views enable scrolling and render scrollbars (if configured)
- [x] Clipped content does not interfere with layout outside the view

**Tests:**

- Unit: non-scrollable view with large content → rendering clips at viewport bounds
- Unit: scrollable view with large content → content accessible via scrolling
- PTY: render clipped content → overflowed content not visible outside bounds

**Notes:**

- Clipping should use Ratatui's `Rect` intersection for bounds checking
- Scrollable views need to calculate content size from children
- Consider separate flags for horizontal and vertical scrolling

---

#### US-7.5: Programmatic Scrolling

**As a** developer
**I want** to scroll to a specific offset or child view programmatically
**So that** I can implement features like "scroll to top" or "scroll into view"

**Acceptance Criteria:**

- [x] Views provide `scroll_to(x, y)` method to set scroll offset directly
- [x] Views provide `scroll_to_child(child_id)` to bring a child into view
- [x] Scrolling to a child centers it in the viewport (if possible)
- [x] Scrolling methods clamp offsets to valid range

**Tests:**

- Unit: call `scroll_to(x, y)` → scroll offset updates
- Unit: call `scroll_to_child()` → child becomes visible in viewport
- Unit: scroll to out-of-bounds offset → clamped to valid range

**Notes:**

- "Scroll into view" may need to calculate child bounds relative to parent
- Consider animation or smooth scrolling (future enhancement)
- May need to invalidate/repaint after programmatic scroll

---

## M8 — Scrollbars

This milestone adds visual scrollbars to scrollable views, allowing users to see scroll position and drag the scrollbar thumb to scroll.

### User Stories

#### US-8.1: Scrollbar Rendering

**As a** user
**I want** to see scrollbars on scrollable views
**So that** I can understand how much content is hidden and my current position

**Acceptance Criteria:**

- [x] Scrollbars appear on the right edge (vertical) and bottom edge (horizontal) of scrollable views
- [x] Scrollbar track spans the full viewport height/width
- [x] Scrollbar thumb size is proportional to `viewport_size / content_size`
- [x] Scrollbar thumb position reflects current scroll offset
- [x] Scrollbars are only rendered when content exceeds viewport size

**Tests:**

- PTY: render a scrollable view with content > viewport → scrollbar visible
- PTY: render a scrollable view with content < viewport → no scrollbar
- PTY: scrollbar thumb size matches proportion of visible content
- Unit: calculate scrollbar thumb size for various content/viewport ratios

**Notes:**

- Scrollbar should use theme colors for track, thumb, and arrows (if present)
- Scrollbar width is typically 1 column; may need to be configurable
- Consider adding arrow buttons at scrollbar ends (↑↓ or ◄►)

---

#### US-8.2: Scrollbar Interaction (Dragging)

**As a** user
**I want** to drag the scrollbar thumb to scroll content
**So that** I can quickly navigate to a specific position

**Acceptance Criteria:**

- [x] Clicking and dragging the scrollbar thumb updates scroll offset proportionally
- [x] Scrollbar thumb follows mouse cursor during drag
- [x] Releasing the mouse button ends the drag operation
- [x] Scroll offset is clamped to valid range during drag

**Tests:**

- PTY: click and drag scrollbar thumb down → content scrolls accordingly
- PTY: drag scrollbar thumb to bottom → content scrolls to end
- Unit: calculate scroll offset from thumb position

**Notes:**

- Need to track mouse drag state (down position, current position)
- Scrollbar thumb should highlight during hover/drag (theme support)
- Consider adding click-on-track behavior (scroll by page)

---

#### US-8.3: Scrollbar Click-on-Track Behavior

**As a** user
**I want** to click on the scrollbar track above/below the thumb
**So that** I can scroll by one page in that direction

**Acceptance Criteria:**

- [x] Clicking scrollbar track above thumb scrolls up by one viewport height
- [x] Clicking scrollbar track below thumb scrolls down by one viewport height
- [x] Clicking on the thumb itself initiates drag (does not scroll)
- [x] Scrolling respects content bounds

**Tests:**

- PTY: click scrollbar track above thumb → content scrolls up by page
- PTY: click scrollbar track below thumb → content scrolls down by page

**Notes:**

- "Above/below" for vertical scrollbar; "left/right" for horizontal scrollbar
- May want to repeat scroll action if mouse is held down (auto-repeat)

---

#### US-8.4: Scrollbar Styling and Configuration

**As a** developer
**I want** to configure scrollbar appearance and behavior
**So that** I can match the application's design and UX requirements

**Acceptance Criteria:**

- [x] Scrollbars support visibility modes: `Always`, `Auto` (hide when not needed), `Never`
- [x] Scrollbar appearance is themeable (track color, thumb color, arrows)
- [x] Scrollbar width/height is configurable
- [x] Scrollbars can be positioned on different edges (e.g., left vs. right for vertical)

**Tests:**

- Unit: set scrollbar visibility to `Never` → no scrollbar rendered
- Unit: set scrollbar visibility to `Auto` → scrollbar appears only when content overflows
- PTY: render scrollbar with custom theme → colors match theme settings

**Notes:**

- Visibility enum: `ScrollbarVisibility::Always | Auto | Never`
- Auto-hide may need hover detection (show scrollbar on mouse hover)
- Consider adding scrollbar position enum: `ScrollbarPosition::Right | Left` (vertical)

---

#### US-8.5: Window Scrolling with Corners Reserved

**As a** user
**I want** scrollbars in windows to avoid all corners for a consistent look-and-feel
**So that** scrollbars have symmetric appearance while keeping window corners usable for resizing

**Acceptance Criteria:**

- [x] Vertical scrollbars in windows start 1 row below the top border and stop 1 row above the bottom border
- [x] Horizontal scrollbars in windows start 1 column after the left border and stop 1 column before the right border
- [x] All four corners are left empty for visual consistency
- [x] Window corners remain usable for resize handles when applicable
- [x] Scrollbars in non-window contexts (e.g., Desktop) use full height/width without corner reservations

**Tests:**

- PTY: scrollable window renders scrollbars and responds to scrolling (`tests/pty_scrolling.rs`, `tests/pty_horizontal_scrolling.rs`)
- Unit: window corners can be used for resize hit-testing (`src/wm/manager.rs`)

**Notes:**

- Window resize handle is typically rendered as `◢` or similar character
- Corner reservations ensure visual symmetry and clean appearance
- Need to detect context (inside window vs. other container) to adjust scrollbar positioning
- May require `ViewContext` to include a `is_inside_window` or `window_kind` flag
- Consider whether maximized or non-resizable windows need corner reservations (likely yes, for consistency)

---

#### US-8.6: Scrollbar Arrow Buttons (Optional)

**As a** user
**I want** arrow buttons at the ends of scrollbars
**So that** I can scroll by small increments with the mouse

**Acceptance Criteria:**

- [ ] Vertical scrollbars render `▲` at top and `▼` at bottom (if enabled)
- [ ] Horizontal scrollbars render `◄` at left and `►` at right (if enabled)
- [ ] Clicking arrow buttons scrolls content by one line/column
- [ ] Arrow buttons are styled to match theme
- [ ] Arrow buttons are optional and can be disabled

**Tests:**

- PTY: click scrollbar up arrow → content scrolls up by one line
- PTY: click scrollbar down arrow → content scrolls down by one line
- Unit: scrollbar with arrows disabled → track spans full height

**Notes:**

- Arrow buttons reduce available track length for thumb
- May want auto-repeat when arrow button is held down
- This is a lower priority enhancement; can be deferred to future milestone

---

## Integration and Testing Strategy for M6-M8

### Unit Tests

- Layout algorithms (VBox, HBox, Grid) with various child sizes and constraints
- Scroll offset clamping and bounds checking
- Scrollbar thumb size and position calculations
- Event routing through view hierarchy

### PTY Integration Tests

- Render nested layouts (VBox inside HBox inside Grid)
- Scroll a list with arrow keys and Page Up/Down
- Click scrollbar thumb and drag to scroll
- Verify scrollbar visibility modes (Always, Auto, Never)
- Verify window scrollbars avoid bottom-right corner

### Manual Validation

- `cargo run --example demo` with new scrollable containers
- Create a demo window with nested layouts and scrollable content
- Test mouse wheel scrolling and keyboard navigation
- Verify smooth rendering and correct clipping

---

## Implementation Notes and Considerations

### Performance

- Layout calculations should be cached and only recomputed when:
  - Parent bounds change (resize)
  - Children are added/removed
  - Child size constraints change
- Avoid re-layout on every frame; use dirty flags
- Scrolling should not trigger full re-layout, only repaint

### API Design

- Consider builder pattern for layout configuration:
  ```rust
  VBox::new()
      .padding(2)
      .spacing(1)
      .add_child(label, Height::Fixed(1))
      .add_child(textbox, Height::Weighted(1.0))
  ```
- Layout containers should implement `View` trait
- Child sizing constraints could use an enum:
  ```rust
  enum SizeConstraint {
      Fixed(u16),
      Weighted(f32),
      Content,
  }
  ```

### View Trait Extensions

The current `View` trait may need extensions:
```rust
pub trait View: Send {
    // Existing methods
    fn handle_event(&mut self, event: &Event, ctx: ViewContext<'_>) -> ViewEventResult;
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>);

    // New methods for hierarchy
    fn children(&self) -> &[Box<dyn View>] { &[] }
    fn children_mut(&mut self) -> &mut Vec<Box<dyn View>> { ... }

    // New methods for scrolling
    fn content_size(&self) -> (u16, u16) { (0, 0) }  // (width, height)
    fn scroll_offset(&self) -> (u16, u16) { (0, 0) }
    fn set_scroll_offset(&mut self, x: u16, y: u16) {}
    fn is_scrollable(&self) -> bool { false }
}
```

### Compatibility

- Existing widgets should continue to work without modification
- Layout containers are opt-in; existing code should not be affected
- Consider adding a `Container` trait separate from `View` for views that manage children

---

## Versioning

These features will be part of the **0.3.0** release:
- View hierarchy and layout management (M6)
- Viewport and scrolling (M7)
- Scrollbars (M8)

Future enhancements (0.4.0+):
- Drag-and-drop between containers
- Docking/splitter panels
- Tree views and collapsible sections
- Smooth scrolling animations
- Touch/gesture support
