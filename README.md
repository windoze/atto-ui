A multi-window TUI application framework for Rust, built on top of Crossterm and Ratatui.
It has following features:
1. Multi-window support, each window can be independently moved, resized, minimized, maximized, and closed. Windows can also be layered on top of each other and maintain correct z-ordering.
2. Built-in window management system that handles window creation, destruction, focus, and z-ordering.
3. Support for various window types, including modal dialogs, tooltips, and floating windows
4. Customizable window decorations, including title bars, borders, control buttons, and drop shadows.
5. Menubars at the top of the application window, with support for nested menus and keyboard shortcuts.
6. Status bars at the bottom of the application window, with support for dynamic content and customizable
7. It should support keyboard and mouse input for window management and interaction.
8. It should support theming and styling of windows and widgets, including colors, and other visual properties.
9. It should support Unicode and wide character rendering for internationalization.
10. It should include common widgets such as buttons, labels, text boxes, checkboxes, radio buttons, lists, and tables that can be used within windows.

The library should have a modular architecture, from a basic view which can be used to build custom windows and widgets, to a high-level window management system that handles all aspects of window behavior and interaction.

The library should expose traits and interfaces that allow developers to easily create custom windows, widgets, and behaviors, while still leveraging the built-in functionality of the framework.

Refer to the [Turbo Vision](https://en.wikipedia.org/wiki/Turbo_Vision) framework for inspiration on design and functionality.

You should use following ways to test the library:
1. Create a test-host app that
    1. allocate a PTY with screen buffer, which can run the TUI application in a pseudo terminal, and capture the screen buffer output for verification.
    2. simulate user input (keyboard, mouse events, and 'paste' event to simulate IME text input) to interact with the TUI application running in the PTY and dump the screen buffer for analysis.
2. Use this test-host app to create automated tests that verify the behavior and functionality of the library, including window management, input handling, rendering, and theming. Verify the screen buffer output against expected results to ensure correctness.

First, you need to create a detailed step-by-step implementation plan for the library, breaking down the development process into manageable tasks and milestones. Each task and milestone should have sufficient tests and validations. Save the plan as `IMPLEMENTATION_PLAN.md`.

Then you should follow the implementation plan to develop the library incrementally, ensuring that each task and milestone is completed and tested before moving on to the next one. Use the test-host app to validate the functionality of the library at each stage of development.

After each task or milestone is completed, update the `IMPLEMENTATION_PLAN.md` file to reflect the progress made and any changes to the plan, git commit the changes with meaningful commit messages.

At the end, you should create a sample TUI application that demonstrates the capabilities of the library, showcasing multi-window management, various window types, customizable decorations, menubars, status bars, input handling, theming, and common widgets. Save this sample application in the `examples/` directory of the project.