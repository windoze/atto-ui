use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::Rect;

use atto_ui_macros::{Automatable, automate_component};
use super::component::{Component, ComponentContext, EventResult};
use super::identifiable::Identifiable;
use super::layout::{EdgeInsets, LayoutParams, Size};
use super::node::{ComponentId, ComponentNode};
use super::scroll::ScrollConfig;
use super::stack::VStack;
use crate::reactive::{Binding, DirtyObserver};

pub type BuilderFn<T, V> = dyn Fn(&T, usize) -> V + Send + Sync;

fn default_foreach_item_layout() -> LayoutParams {
    LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    }
}

/// ForEach 容器 - 根据数据列表动态生成子组件
///
/// ForEach 是可组合 API 的核心容器之一，用于高效地渲染数据列表。
/// 它会根据绑定的数据源自动创建、更新和删除子组件。
///
/// # 基础用法
///
/// ```rust,no_run
/// use atto_ui::composable::{ForEach, Text};
/// use atto_ui::reactive::Property;
///
/// let items = Property::new(vec!["Apple", "Banana", "Cherry"]);
///
/// let list = ForEach::new(items.binding(), |item, idx| {
///     Text::new(format!("{idx}. {item}"))
/// });
/// ```
///
/// # 状态绑定
///
/// 数据结构可以包含反应式属性，直接绑定到控件：
///
/// ```rust,no_run
/// use atto_ui::composable::{ForEach, HStack, Checkbox, Text};
/// use atto_ui::reactive::Property;
///
/// #[derive(Clone)]
/// struct TodoItem {
///     text: String,
///     completed: Property<bool>,
/// }
///
/// impl PartialEq for TodoItem {
///     fn eq(&self, other: &Self) -> bool {
///         self.text == other.text
///     }
/// }
///
/// let todos = Property::new(vec![TodoItem {
///     text: "Example".to_string(),
///     completed: Property::new(false),
/// }]);
///
/// let list = ForEach::new(todos.binding(), |todo, _| {
///     HStack::new()
///         .child(Checkbox::new("", todo.completed.binding()))
///         .child(Text::new(&todo.text))
/// });
/// ```
///
/// # 回调处理
///
/// 使用闭包捕获传递元素上下文到回调：
///
/// ```rust,no_run
/// use atto_ui::composable::{ForEach, Button};
/// use atto_ui::reactive::Property;
/// use std::sync::Arc;
///
/// #[derive(Clone, PartialEq)]
/// struct User {
///     id: usize,
///     name: String,
/// }
///
/// let users = Property::new(vec![User {
///     id: 1,
///     name: "Alice".to_string(),
/// }]);
/// let on_click = Arc::new(|id: usize| println!("Clicked {id}"));
///
/// let list = ForEach::new(users.binding(), move |user, _| {
///     let on_click = on_click.clone();
///     let user_id = user.id;
///
///     Button::new(user.name.clone())
///         .on_click(move || on_click(user_id))
/// });
/// ```
#[derive(Automatable)]
pub struct ForEach<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: Component + 'static,
{
    data: Binding<Vec<T>>,
    builder: Arc<BuilderFn<T, V>>,
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scrollable: Binding<bool>,
    scroll_config: Binding<ScrollConfig>,
    cached_view: VStack,
    data_observer: DirtyObserver,
    _phantom: PhantomData<V>,
}

impl<T, V> ForEach<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: Component + 'static,
{
    /// 创建 ForEach 容器
    ///
    /// # 参数
    /// - `data`: 绑定到数据列表的 Binding
    /// - `builder`: 为每个元素构建组件的闭包，接收 (元素引用, 索引)
    ///
    /// # 示例
    /// ```rust,no_run
    /// use atto_ui::composable::{ForEach, Text};
    /// use atto_ui::reactive::Property;
    ///
    /// let items = Property::new(vec!["Apple", "Banana", "Cherry"]);
    ///
    /// let list = ForEach::new(items.binding(), |item, idx| {
    ///     Text::new(format!("{idx}. {item}"))
    /// });
    /// ```
    pub fn new<F>(data: Binding<Vec<T>>, builder: F) -> Self
    where
        F: Fn(&T, usize) -> V + Send + Sync + 'static,
    {
        let spacing: Binding<u16> = 0u16.into();
        let padding: Binding<EdgeInsets> = EdgeInsets::ZERO.into();
        let scrollable: Binding<bool> = false.into();
        let scroll_config: Binding<ScrollConfig> = ScrollConfig::default().into();
        let cached_view = VStack::new()
            .with_padding(padding.clone())
            .with_spacing(spacing.clone())
            .with_scrollable(scrollable.clone())
            .with_scroll_config(scroll_config.clone());

        let mut view = Self {
            data,
            builder: Arc::new(builder),
            spacing,
            padding,
            scrollable,
            scroll_config,
            cached_view,
            data_observer: DirtyObserver::default(),
            _phantom: PhantomData,
        };
        view.data_observer = view.data.dirty_observer();
        view.rebuild_children();
        view
    }

    /// 设置子元素间距
    pub fn spacing(mut self, spacing: impl Into<Binding<u16>>) -> Self {
        self.spacing = spacing.into();
        self.rebuild_view();
        self
    }

    /// 设置内边距（所有边）
    pub fn padding(mut self, padding: u16) -> Self {
        self.padding = EdgeInsets::all(padding).into();
        self.rebuild_view();
        self
    }

    /// 设置内边距（自定义 EdgeInsets）
    pub fn padding_insets(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.padding = padding.into();
        self.rebuild_view();
        self
    }

    /// 启用滚动
    pub fn scrollable(mut self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.scrollable = scrollable.into();
        self.rebuild_view();
        self
    }

    /// 设置滚动配置
    pub fn scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.scroll_config = config.into();
        self.rebuild_view();
        self
    }

    /// 创建使用 Identifiable 的优化 ForEach
    pub fn with_id(self) -> ForEachIdentifiable<T, V>
    where
        T: Identifiable,
        T::Id: Hash + Eq + Send + Sync,
    {
        let items = self.data.get();
        let mut cached_items = HashMap::with_capacity(items.len());
        let mut cached_ids = Vec::with_capacity(items.len());
        for item in items.iter() {
            let id = item.id();
            cached_items.insert(id.clone(), item.clone());
            cached_ids.push(id);
        }

        let mut view = ForEachIdentifiable {
            data: self.data,
            builder: self.builder,
            scroll_config: self.scroll_config,
            cached_view: self.cached_view,
            cached_items,
            cached_ids,
            data_observer: DirtyObserver::default(),
            _phantom: PhantomData,
        };
        view.data_observer = view.data.dirty_observer();
        view
    }

    fn rebuild_view(&mut self) {
        self.cached_view = VStack::new()
            .with_padding(self.padding.clone())
            .with_spacing(self.spacing.clone())
            .with_scrollable(self.scrollable.clone())
            .with_scroll_config(self.scroll_config.clone());
        self.rebuild_children();
    }

    fn rebuild_children(&mut self) {
        let items = self.data.get();
        let mut children = Vec::with_capacity(items.len());
        for (idx, item) in items.iter().enumerate() {
            let child_view = (self.builder)(item, idx);
            children.push(
                ComponentNode::new(Box::new(child_view)).with_layout(default_foreach_item_layout()),
            );
        }

        self.cached_view.replace_children(children);
    }
}

#[automate_component]
impl<T, V> Component for ForEach<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: Component + 'static,
{
    fn automation_focused_child(&self) -> Option<ComponentId> {
        self.cached_view.automation_focused_child()
    }

    fn min_width(&self) -> u16 {
        self.cached_view.min_width()
    }

    fn min_height(&self) -> u16 {
        self.cached_view.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.cached_view.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.cached_view.desired_height()
    }

    fn children(&self) -> &[ComponentNode] {
        self.cached_view.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.cached_view.children_mut()
    }

    fn is_scrollable(&self) -> bool {
        self.cached_view.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.cached_view.content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.cached_view.scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.cached_view.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config.get()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.cached_view.set_scroll_offset(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        self.cached_view.scroll_to_child(child_id);
    }

    fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        if self.data.check_dirty(&mut self.data_observer) {
            self.rebuild_children();
        }

        self.cached_view.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if self.data.check_dirty(&mut self.data_observer) {
            self.rebuild_children();
        }

        self.cached_view.draw(frame, area, ctx);
    }

    fn is_focusable(&self) -> bool {
        self.cached_view.is_focusable()
    }
}

/// ForEach 的优化变体 - 使用 Identifiable trait 进行增量更新
///
/// 此结构通过 ID 跟踪列表元素，实现了视图缓存和差异更新。
/// 当数据变化时，只有新增、删除或修改的元素会重建视图。
#[derive(Automatable)]
pub struct ForEachIdentifiable<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: Component + 'static,
{
    data: Binding<Vec<T>>,
    builder: Arc<BuilderFn<T, V>>,
    scroll_config: Binding<ScrollConfig>,
    cached_view: VStack,
    cached_items: HashMap<T::Id, T>,
    cached_ids: Vec<T::Id>,
    data_observer: DirtyObserver,
    _phantom: PhantomData<V>,
}

impl<T, V> ForEachIdentifiable<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: Component + 'static,
{
    fn reconcile_children(&mut self) {
        let items = self.data.get();

        let old_ids = std::mem::take(&mut self.cached_ids);
        let old_children = {
            let children = self
                .cached_view
                .children_mut()
                .expect("VStack should expose children_mut");
            std::mem::take(children)
        };

        let mut old_by_id: HashMap<T::Id, ComponentNode> =
            HashMap::with_capacity(old_children.len());
        if old_ids.len() == old_children.len() {
            for (id, node) in old_ids.into_iter().zip(old_children) {
                old_by_id.insert(id, node);
            }
        }

        let old_cached_items = std::mem::take(&mut self.cached_items);
        let mut new_cached_items = HashMap::with_capacity(items.len());
        let mut new_children = Vec::with_capacity(items.len());
        let mut new_ids = Vec::with_capacity(items.len());

        for (idx, item) in items.iter().enumerate() {
            let id = item.id();
            let node = match old_by_id.remove(&id) {
                Some(mut node) => {
                    node.layout = default_foreach_item_layout();

                    let needs_rebuild = old_cached_items.get(&id) != Some(item);
                    if needs_rebuild {
                        node.view = Box::new((self.builder)(item, idx));
                    }
                    node
                }
                None => {
                    let child_view = (self.builder)(item, idx);
                    ComponentNode::new(Box::new(child_view))
                        .with_layout(default_foreach_item_layout())
                }
            };

            new_cached_items.insert(id.clone(), item.clone());
            new_ids.push(id);
            new_children.push(node);
        }

        self.cached_items = new_cached_items;
        self.cached_ids = new_ids;
        self.cached_view.replace_children(new_children);
    }
}

#[automate_component]
impl<T, V> Component for ForEachIdentifiable<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: Component + 'static,
{
    fn automation_focused_child(&self) -> Option<ComponentId> {
        self.cached_view.automation_focused_child()
    }

    fn min_width(&self) -> u16 {
        self.cached_view.min_width()
    }

    fn min_height(&self) -> u16 {
        self.cached_view.min_height()
    }

    fn desired_width(&self) -> Option<u16> {
        self.cached_view.desired_width()
    }

    fn desired_height(&self) -> Option<u16> {
        self.cached_view.desired_height()
    }

    fn children(&self) -> &[ComponentNode] {
        self.cached_view.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ComponentNode>> {
        self.cached_view.children_mut()
    }

    fn is_scrollable(&self) -> bool {
        self.cached_view.is_scrollable()
    }

    fn content_size(&self) -> (u16, u16) {
        self.cached_view.content_size()
    }

    fn scroll_offset(&self) -> (u16, u16) {
        self.cached_view.scroll_offset()
    }

    fn viewport_size(&self) -> (u16, u16) {
        self.cached_view.viewport_size()
    }

    fn scroll_config(&self) -> ScrollConfig {
        self.scroll_config.get()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.cached_view.set_scroll_offset(x, y);
    }

    fn scroll_to_child(&mut self, child_id: ComponentId) {
        self.cached_view.scroll_to_child(child_id);
    }

    fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
        ctx: ComponentContext<'_>,
    ) -> EventResult {
        if self.data.check_dirty(&mut self.data_observer) {
            self.reconcile_children();
        }

        self.cached_view.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ComponentContext<'_>) {
        if self.data.check_dirty(&mut self.data_observer) {
            self.reconcile_children();
        }

        self.cached_view.draw(frame, area, ctx);
    }

    fn is_focusable(&self) -> bool {
        self.cached_view.is_focusable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composable::Text;
    use crate::composable::{ScrollbarHost, TabMode};
    use crate::reactive::Property;
    use crate::theme::Theme;
    use crate::wm::WindowId;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn draw_component(view: &mut dyn Component, area: Rect, scrollbar_host: ScrollbarHost) {
        let theme = Theme::dark();
        let ctx = ComponentContext {
            theme: &theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host,
            tab_mode: TabMode::Cycle,
        };

        let backend = TestBackend::new(area.width.max(1), area.height.max(1));
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| view.draw(f, area, ctx)).expect("draw");
    }

    #[test]
    fn test_foreach_empty_list() {
        let data = Property::new(Vec::<String>::new());
        let for_each = ForEach::new(data.binding(), |item, _idx| Text::new(item.clone()));

        let children = for_each.children();
        assert_eq!(children.len(), 0);
    }

    #[test]
    fn test_foreach_simple_list() {
        let data = Property::new(vec!["Apple".to_string(), "Banana".to_string()]);
        let for_each = ForEach::new(data.binding(), |item, _idx| Text::new(item.clone()));

        let children = for_each.children();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_foreach_with_index() {
        let data = Property::new(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        let for_each = ForEach::new(data.binding(), |item, idx| {
            Text::new(format!("{idx}. {item}"))
        });

        let children = for_each.children();
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn test_foreach_dynamic_update() {
        let data = Property::new(vec!["A".to_string()]);
        let mut for_each = ForEach::new(data.binding(), |item, _idx| Text::new(item.clone()));

        draw_component(
            &mut for_each,
            Rect::new(0, 0, 10, 5),
            ScrollbarHost::Component,
        );
        let children = for_each.children();
        assert_eq!(children.len(), 1);

        data.set(vec!["A".to_string(), "B".to_string()]);

        draw_component(
            &mut for_each,
            Rect::new(0, 0, 10, 5),
            ScrollbarHost::Component,
        );
        let children = for_each.children();
        assert_eq!(children.len(), 2);

        data.set(Vec::new());
        draw_component(
            &mut for_each,
            Rect::new(0, 0, 10, 5),
            ScrollbarHost::Component,
        );
        let children = for_each.children();
        assert_eq!(children.len(), 0);
    }

    #[test]
    fn test_foreach_builder_methods() {
        let data = Property::new(vec!["A".to_string()]);
        let for_each = ForEach::new(data.binding(), |item, _idx| Text::new(item.clone()))
            .spacing(2)
            .padding(1)
            .scrollable(true);

        assert_eq!(for_each.spacing.get(), 2);
        assert_eq!(for_each.padding.get(), EdgeInsets::all(1));
        assert!(for_each.scrollable.get());
    }

    #[test]
    fn test_foreach_with_id() {
        #[derive(Clone, PartialEq)]
        struct Item {
            id: usize,
            value: String,
        }

        impl Identifiable for Item {
            type Id = usize;
            fn id(&self) -> Self::Id {
                self.id
            }
        }

        let data = Property::new(vec![
            Item {
                id: 1,
                value: "A".to_string(),
            },
            Item {
                id: 2,
                value: "B".to_string(),
            },
        ]);

        let for_each =
            ForEach::new(data.binding(), |item, _idx| Text::new(item.value.clone())).with_id();

        let children = for_each.children();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn foreach_scroll_metrics_reflect_content_rows() {
        let data = Property::new((0..20).map(|i| format!("Item {i}")).collect::<Vec<_>>());
        let mut for_each = ForEach::new(data.binding(), |item, idx| {
            Text::new(format!("{idx}: {item}"))
        })
        .scrollable(true);

        draw_component(
            &mut for_each,
            Rect::new(0, 0, 10, 5),
            ScrollbarHost::Component,
        );

        let content = for_each.content_size();
        assert!(content.1 > 0, "content height should be > 0");
    }

    #[test]
    fn foreach_id_reuses_cached_components() {
        #[derive(Clone, PartialEq)]
        struct Item {
            id: usize,
            value: String,
        }

        impl Identifiable for Item {
            type Id = usize;
            fn id(&self) -> Self::Id {
                self.id
            }
        }

        let data = Property::new(vec![Item {
            id: 1,
            value: "A".to_string(),
        }]);

        let build_count = Arc::new(AtomicUsize::new(0));
        let build_count_ref = Arc::clone(&build_count);
        let mut for_each = ForEach::new(data.binding(), move |item, _| {
            build_count_ref.fetch_add(1, Ordering::SeqCst);
            Text::new(item.value.clone())
        })
        .with_id();

        draw_component(
            &mut for_each,
            Rect::new(0, 0, 10, 5),
            ScrollbarHost::Component,
        );
        assert_eq!(build_count.load(Ordering::SeqCst), 1);

        data.set(vec![Item {
            id: 1,
            value: "A".to_string(),
        }]);

        draw_component(
            &mut for_each,
            Rect::new(0, 0, 10, 5),
            ScrollbarHost::Component,
        );
        assert_eq!(build_count.load(Ordering::SeqCst), 1);

        data.set(vec![Item {
            id: 1,
            value: "B".to_string(),
        }]);

        draw_component(
            &mut for_each,
            Rect::new(0, 0, 10, 5),
            ScrollbarHost::Component,
        );
        assert_eq!(build_count.load(Ordering::SeqCst), 2);
    }
}
