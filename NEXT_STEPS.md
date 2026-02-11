# Next steps

## Stage 1, low complexity additions:

### Architectural changes:
- [x] Integrate a channel into app event loop to handle async events from other threads, such as network events or timers, and dispatch them to the main thread for processing. Details are in ASYNC.md. No need to introduce `tokio` or other async runtimes, just use standard library threads and channels.
- [x] Automation and introspection support:
  - Each component (menu, window, layout, widget, etc.) can have an optional unique identifier (string) that can be used to reference it programmatically.
  - Implement a method to traverse the widget tree from the root desktop down to leaf components.
  - Provide a way to query and modify component binding properties (e.g., text, value, visibility) programmatically.
  - Provide a way to simulate user interactions (e.g., button clicks, text input) programmatically for automation purposes.

### Updates to existing components:
- [x] ListBox should show scrollbars when content overflows, vertical scroll should be enabled by default and horizontal scroll disabled by default
- [x] TableView should show scrollbars when content overflows, vertical scroll should keep the header row visible, that is, scroll the body rows only
- [x] TextBox should support:
  - An optional placeholder text when empty, displayed in a separated style
  - Text selection, both mouse and keyboard based
  - Copy/Cut/Paste operations with standard keybindings
  - Double click to select word and triple click to select line
  - Indicators at the start and end of the visible area when the text is out of view on each side, you can use scrollbar arrows

### New widgets:
- [x] ProgressBar: A horizontal progress bar with configurable min, max, and current values, supports optional text display inside the bar
- [x] Slider: A horizontal slider control with draggable thumb to select a value within a specified range
- [x] Spinner: A loading animation widget that can be shown when an operation is in progress, supports different styles of spinner (e.g., dots, bars, circles) and customizable animation speed. Animation is driven by the global ticker/timer wheel. The animation effects to be supported:
  - Animated icon (like "⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏"), a list of characters should be provided to form the animation frames, the lib can have some built-in styles
  - String with flowing colors, e.g., "Loading..." with colors flowing from left to right, the speed and color scheme should be customizable
  - Above two can be combined together with options to customize the layout (icon on left/right of text, spacing, etc)
- [x] Styled Label: A text label that supports different styles such as bold, italic, underline, it accepts markdown-like syntax for styling, include:
  - `**bold**` for bold text
  - `*italic*` for italic text
  - `__underline__` for underlined text
  - `~~strikethrough~~` for strikethrough text
  - `[link text](url)` for hyperlinks, which can be clicked to call a callback function with the URL as parameter
  - **No other markdown syntax needs to be supported**, and the style processing can be basic, e.g., no nested styles.

### New containers:
- [x] TabView: A container widget that allows switching between multiple tabs, each tab is a container and containing its own set of child widgets, supports adding/removing tabs dynamically and selecting tabs programmatically. Tab headers can be put on top or bottom.
- [x] TabWindow: A top-level window with tabs, each tab contains its own set of child widgets, supports adding/removing tabs dynamically and selecting tabs programmatically. The window title area needs to be customized to show the tabs and allow switching between them, and the scrollbar visibility should depend on the content of the selected tab. The window title should be look like " | Tab0 | Tab1  Tab2  Tab3 | " with the active tab highlighted, which is 'Tab2' in this example.

## Stage 2, relative complexity additions:

### Architectural changes:
- [ ] Multi-stroke shortcut support: Extend the shortcut handling system to support multi-stroke shortcuts (e.g., "Ctrl+K, Ctrl+C" to trigger a command). This involves updating the event handling logic to recognize sequences of key presses and maintain state between strokes.

### Updates to existing components:
- [ ] Editor: 
  - (TBD)

### New widgets:
- [x] Chat message list: A widget to display a list of chat messages, supporting different message types (text, files), timestamps, in-progress animation, sender information, and use MarkdownViewer to show message content. It should support scrolling and loading more messages when scrolling to the top, and support accessing/updating each message programmatically.
- [ ] File tree: A hierarchical file explorer widget that displays files and directories in a tree structure, supports expanding/collapsing directories, selecting files, and basic file operations like rename and delete, with optional filter to decide which files/directories to show and what glyphs/icons to use for different file types.
- [x] Terminal emulator: A widget that emulates a terminal, supporting basic terminal features like text input/output, ANSI escape codes for colors and cursor movement, and scrollback buffer.

