# TODO

## Mis-designs

- [ ] Layered event handling, e.g., when a modal window is open, only that window should receive input events. Events may bubble up to parent views/windows if not handled. A typical flow is:
    1. Focused view receives event first
    2. If not handled, focused window receives event
    3. If not handled, application receives event
    4. Some events sinks may choose to stop event propagation, e.g., modal windows.

## Bugs

- [ ] Drop down menu should be above other content, now it is below window
- [ ] Modal window (include dropdown menus) should grab the input so global shortcuts do not trigger actions when modal is open
- [ ] Drop shadow missing the bottom right corner
- [ ] View/Window should have a solid background by default, now it is transparent, that is, the content behind it is visible if foreground has no text.

## Missing Features and Improvements

- [ ] Mouse support, include:
    - [ ] Clicking on menu bar items to open drop down menus
    - [ ] Clicking on drop down menu items to trigger actions or open sub-menus
    - [ ] Clicking on focusable non-focused views to give them focus
    - [ ] Clicking on a control in a focused view should trigger the control's action
    - [ ] Clicking in text input fields to set the cursor position
    - [ ] Clicking and dragging to select text in text input fields
- [ ] Focused window should have double-line border
- [ ] Window should be resizable by mouse dragging the bottom right corner
- [ ] Window closing hooks, the user should be able to register a callback that is called when the user tries to close the window, e.g., to pop up a "Are you sure?" dialog.
- [ ] Menu item in the menu bar should also have a keyboard shortcut, e.g. "Alt+F" to open the "File" menu.
