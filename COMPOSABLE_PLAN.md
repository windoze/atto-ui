# Composable Component Refactor Plan

## 1. Current component layers and friction

- View trait (src/view.rs): imperative draw + event, min/desired sizing, scroll hooks.
- Views module (src/views/*): ViewNode tree, layout params, border/scroll wrappers, ScrollView + ScrollContent, ControlView adapter.
- Widgets module (src/widgets/*): Control trait + Form container, separate event/result types, separate focus handling.
- Declarative module (src/declarative/*): DeclarativeView, primitives, layout builders, ViewAdapter bridge, and separate layout engines (stack_view, grid_view).
- Window manager and app layers assume View as the root component.

Pain points:

- Multiple component APIs (View, Control, DeclarativeView, ScrollContent) with adapters between them.
- Duplicate layout and focus code across declarative stack/grid views and view wrappers.
- Two event result types (ViewEventResult vs ControlOutcome/FormAction) and different context models.
- Scroll behavior split across View hooks, ScrollView, and window chrome.
- Declarative tree does not rebuild on state changes unless individual widgets manage bindings.

## 2. Target composable design (single component model)

### 2.1 Core traits and types

Introduce a single Component trait and supporting types under a new module, for example:

```
pub trait Component: Send {
    fn is_focusable(&self) -> bool { false }
    fn focus_first(&mut self) -> bool { self.is_focusable() }
    fn focus_last(&mut self) -> bool { self.is_focusable() }
    fn layout(&mut self, ctx: &mut LayoutCtx, constraints: Constraints) -> LayoutNode;
    fn event(&mut self, ctx: &mut EventCtx, event: &Event) -> EventResult;
    fn draw(&mut self, ctx: &mut DrawCtx, area: Rect);
    fn children(&self) -> &[ComponentNode] { &[] }
    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> { None }
}
```

Key supporting types:

- ComponentId (replaces ViewId) with stable identity and optional keying for diffing.
- ComponentNode (replaces ViewNode): holds child component + layout params + bounds.
- LayoutParams, EdgeInsets, Size, Align, Anchor (move from src/views/layout.rs).
- Constraints and LayoutNode: explicit measurement output for layout engines.
- ComponentContext (merges ViewContext + theme + focus + scroll host).
- EventResult: merge ViewEventResult and ControlOutcome/FormAction into one action model.

### 2.2 Unified capabilities

- Focus: central FocusManager with FocusScope and tab ordering, replaces Form + custom focus logic.
- Scroll: unified ScrollState and Scrollbars API used by all components (container or leaf).
- Window actions: component events can request actions (close window, open modal, etc).
- Reactive updates: integrate Binding and DirtyObserver to trigger layout or draw invalidation.

### 2.3 Composable API

Make composition the primary public surface:

- A single "composable" module that exports containers and primitives.
- Builder-style API (existing style) stays but targets Component.
- Macro support (view_builder) updated to emit new composable types.
- Optional Element enum for declarative trees if needed for diffing and keyed updates.

## 3. Migration plan (phased, no code removal until replacement is in place)

### Phase 0: Baseline and design doc

- Document the new Component API and its required behaviors.
- Define exact mapping of old types to new ones (see table below).
- Freeze current tests and snapshot outputs for reference.

### Phase 1: New core module and adapters

Goal: compile a new composable core alongside old APIs.

Work items:

- Add `src/composable/` (or `src/component/`) with:
  - Component trait, ComponentId, ComponentNode.
  - Layout types: LayoutParams, EdgeInsets, Size, Align, Anchor.
  - Contexts: LayoutCtx, EventCtx, DrawCtx, ComponentContext.
  - EventResult and ComponentAction.
- Build a ComponentTree runtime that:
  - Handles layout, focus, and event routing (capture/bubble optional).
  - Tracks bounds for hit testing and scrollbars.
- Add adapters:
  - ViewAsComponent: wrap Box<dyn View> inside Component.
  - ControlAsComponent: wrap Box<dyn Control> inside Component.
  - ComponentAsView (temporary): allow WM to stay on View until final port.

### Phase 2: Consolidate layout and container engines

Goal: remove duplicated layout logic and establish a single container implementation.

Work items:

- Move layout math from src/views/layout.rs into composable core.
- Extract reusable layout engine used by Stack and Grid components.
- Implement composable containers:
  - VStack, HStack, Grid, ZStack (if needed for overlays).
  - Frame, Padding, Align, Anchor.
- Replace declarative stack_view/grid_view with these containers.
- Ensure scrollable containers use the unified scroll APIs.

### Phase 3: Unify scrolling and wrapper views

Goal: replace ScrollView/ScrollContent and BorderView with composable wrappers.

Work items:

- Create ScrollContainer component:
  - Owns ScrollState, scrollbars, wheel handling, and viewport math.
  - Can host a child component or a virtualized content delegate.
- Create Border component with optional scrollbars hosting.
- Port WindowMinSizeView logic into a composable wrapper component.
- Update snapshot_virtual_scroll_app to use the new scroll container API.

### Phase 4: Migrate widgets to Component

Goal: widgets are first-class components, remove Control trait.

Work items:

- Convert each widget in src/widgets/* to implement Component directly:
  - Button, TextBox, Checkbox, RadioGroup, Label, ListBox, TableView.
- Replace ControlOutcome/FormAction with EventResult.
- Rework Form into a FocusScope + Stack container (or remove if redundant).
- Remove ControlView usage in new code paths.

### Phase 5: Migrate declarative API and macros

Goal: declarative layer becomes the composable surface, no separate DeclarativeView trait.

Work items:

- Replace DeclarativeView with Component or an Element builder that produces Component trees.
- Update `ViewAdapter` to become a no-op or remove it entirely.
- Update view_builder macro to build composable components instead of declarative views.
- Update tests:
  - tests/declarative_primitives.rs
  - tests/declarative_vstack.rs
  - tests/macro_view_builder.rs

### Phase 6: Port window manager and app integration

Goal: WM and app use Component as the root, remove View usage.

Work items:

- Update src/wm/window.rs to hold Box<dyn Component>.
- Replace ViewContext usage in src/wm/manager.rs with ComponentContext.
- Move event routing to ComponentTree runtime; WM focuses on window chrome only.
- Update Desktop/app to use the new APIs.

### Phase 7: Port complex views and dialogs

Goal: convert high-level components to the new model.

Work items:

- Port EditorView to Component with unified scroll handling.
- Port dialogs (FileDialog) to composable containers and FocusScope.
- Update examples and snapshot apps that rely on old declarative/view APIs.

### Phase 8: Remove old layers and cleanup

Goal: delete legacy APIs and reduce surface area.

Work items:

- Remove src/view.rs, src/views/*, and widgets::Control.
- Remove src/declarative/view.rs and ViewAdapter.
- Update src/lib.rs exports to the new composable module.
- Update README and docs to describe the new model.
- Update IMPLEMENTATION_PLAN.md milestone status if appropriate.

## 4. Mapping of old to new components

- View trait -> Component trait
- ViewNode/ViewId -> ComponentNode/ComponentId
- Control trait -> Component leaf widgets
- Form -> FocusScope + Stack container
- BorderView -> Border component
- ScrollView/ScrollContent -> ScrollContainer + optional virtualized content interface
- DeclarativeView -> Component builder or Element-based composition
- ViewAdapter -> removed (or thin compatibility wrapper during migration)

## 5. Testing and validation

Per phase, add or update tests:

- Unit tests for layout math, focus traversal, scroll math in composable core.
- PTY integration tests updated to use new APIs.
- Snapshot apps updated to use new composable components; verify buffer output.
- Editor and dialog behavior validated via existing PTY tests.

Suggested checkpoints:

1) New composable core compiles with adapters, no behavior change.
2) Layout containers and scroll wrappers match existing snapshots.
3) Widgets and declarative API migrated, examples still render.
4) WM switched to Component, old View removed.
5) All tests pass and old modules are deleted.

## 6. Open questions to resolve before implementation

- Do we need an Element tree with diffing, or is a ComponentNode tree sufficient?
- How should layout measurement be expressed (constraints vs min/desired size)?
- Where should scrollbars be rendered by default (component vs window chrome)?
- How will keying work in ForEach and other dynamic lists?
- Do we keep a separate "virtualized content" trait or make it a Component?
