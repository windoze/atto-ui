# Implementation Plan

## Milestones

### 1. Composable Component Refactor (COMPOSABLE_PLAN)
Status: Completed

- [x] Introduce composable core (`Component`, layout types, contexts, event result).
- [x] Consolidate layout containers (VStack/HStack/Grid) and scrolling wrappers.
- [x] Migrate widgets to implement `Component` directly.
- [x] Update window manager, app, dialogs, editor integration to use `Component`.
- [x] Update macros, tests, examples, and demos to the composable API.
- [x] Remove legacy `view`, `views`, and `declarative` modules.
- [x] Refresh documentation for the composable architecture.
