use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::reactive::{Binding, DirtyObserver};
use crate::view::{View, ViewContext};
use crate::views::{EdgeInsets, LayoutParams, ScrollConfig, Size, ViewNode};

use super::identifiable::Identifiable;
use super::stack_view::VStackView;
use super::view::{DeclarativeView, EmptyView};

pub type BuilderFn<T, V> = dyn Fn(&T, usize) -> V + Send + Sync;

fn default_foreach_item_layout() -> LayoutParams {
    LayoutParams {
        height: Size::Content,
        ..LayoutParams::default()
    }
}

/// ForEach 容器 - 根据数据列表动态生成子视图
///
/// ForEach 是声明式 API 的核心容器之一，用于高效地渲染数据列表。
/// 它会根据绑定的数据源自动创建、更新和删除子视图。
///
/// # 基础用法
///
/// ```rust,no_run
/// use chatty::declarative::{ForEach, Text};
/// use chatty::reactive::Property;
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
/// use chatty::declarative::{ForEach, HStack, Checkbox, Text};
/// use chatty::reactive::Property;
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
/// use chatty::declarative::{ForEach, Button};
/// use chatty::reactive::Property;
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
pub struct ForEach<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: DeclarativeView + 'static,
{
    data: Binding<Vec<T>>,
    builder: Arc<BuilderFn<T, V>>,
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scrollable: Binding<bool>,
    scroll_config: Binding<ScrollConfig>,
    _phantom: PhantomData<V>,
}

impl<T, V> ForEach<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: DeclarativeView + 'static,
{
    /// 创建 ForEach 容器
    ///
    /// # 参数
    /// - `data`: 绑定到数据列表的 Binding
    /// - `builder`: 为每个元素构建视图的闭包，接收 (元素引用, 索引)
    ///
    /// # 示例
    /// ```rust,no_run
    /// use chatty::declarative::{ForEach, Text};
    /// use chatty::reactive::Property;
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
        Self {
            data,
            builder: Arc::new(builder),
            spacing: 0u16.into(),
            padding: EdgeInsets::ZERO.into(),
            scrollable: false.into(),
            scroll_config: ScrollConfig::default().into(),
            _phantom: PhantomData,
        }
    }

    /// 设置子元素间距
    ///
    /// # 示例
    /// ```rust,no_run
    /// # use chatty::declarative::{ForEach, Text};
    /// # use chatty::reactive::Property;
    /// # let items = Property::new(vec!["A", "B"]);
    /// let list = ForEach::new(items.binding(), |item, _| Text::new(*item))
    ///     .spacing(1);
    /// ```
    pub fn spacing(mut self, spacing: impl Into<Binding<u16>>) -> Self {
        self.spacing = spacing.into();
        self
    }

    /// 设置内边距（所有边）
    ///
    /// # 示例
    /// ```rust,no_run
    /// # use chatty::declarative::{ForEach, Text};
    /// # use chatty::reactive::Property;
    /// # let items = Property::new(vec!["A", "B"]);
    /// let list = ForEach::new(items.binding(), |item, _| Text::new(*item))
    ///     .padding(2);
    /// ```
    pub fn padding(mut self, padding: u16) -> Self {
        self.padding = EdgeInsets::all(padding).into();
        self
    }

    /// 设置内边距（自定义 EdgeInsets）
    ///
    /// # 示例
    /// ```rust,no_run
    /// # use chatty::declarative::{ForEach, Text, EdgeInsets};
    /// # use chatty::reactive::Property;
    /// # let items = Property::new(vec!["A", "B"]);
    /// let list = ForEach::new(items.binding(), |item, _| Text::new(*item))
    ///     .padding_insets(EdgeInsets::symmetric(1, 2));
    /// ```
    pub fn padding_insets(mut self, padding: impl Into<Binding<EdgeInsets>>) -> Self {
        self.padding = padding.into();
        self
    }

    /// 启用滚动
    ///
    /// # 示例
    /// ```rust,no_run
    /// # use chatty::declarative::{ForEach, Text};
    /// # use chatty::reactive::Property;
    /// # let items = Property::new(vec!["A", "B"]);
    /// let list = ForEach::new(items.binding(), |item, _| Text::new(*item))
    ///     .scrollable(true);
    /// ```
    pub fn scrollable(mut self, scrollable: impl Into<Binding<bool>>) -> Self {
        self.scrollable = scrollable.into();
        self
    }

    /// 设置滚动配置
    ///
    /// # 示例
    /// ```rust,no_run
    /// # use chatty::declarative::{ForEach, Text};
    /// # use chatty::reactive::Property;
    /// # use chatty::views::ScrollConfig;
    /// # let items = Property::new(vec!["A", "B"]);
    /// let list = ForEach::new(items.binding(), |item, _| Text::new(*item))
    ///     .scrollable(true)
    ///     .scroll_config(ScrollConfig::default());
    /// ```
    pub fn scroll_config(mut self, config: impl Into<Binding<ScrollConfig>>) -> Self {
        self.scroll_config = config.into();
        self
    }

    /// 构建当前子元素列表（内部辅助方法）
    fn build_children(&self) -> Vec<V> {
        let items = self.data.get();
        items
            .iter()
            .enumerate()
            .map(|(idx, item)| (self.builder)(item, idx))
            .collect()
    }

    /// 创建使用 Identifiable 的优化 ForEach
    ///
    /// 此方法返回一个使用 ID 跟踪的 ForEach 变体，可以实现增量更新，
    /// 避免不必要的视图重建，适合大列表或频繁更新的场景。
    ///
    /// # 示例
    /// ```rust,no_run
    /// use chatty::declarative::{ForEach, Text, Identifiable};
    /// use chatty::reactive::Property;
    ///
    /// #[derive(Clone, PartialEq)]
    /// struct User {
    ///     id: usize,
    ///     name: String,
    /// }
    ///
    /// impl Identifiable for User {
    ///     type Id = usize;
    ///     fn id(&self) -> Self::Id {
    ///         self.id
    ///     }
    /// }
    ///
    /// let users = Property::new(vec![
    ///     User { id: 1, name: "Alice".to_string() },
    ///     User { id: 2, name: "Bob".to_string() },
    /// ]);
    ///
    /// let list = ForEach::new(users.binding(), |user, _| {
    ///     Text::new(&user.name)
    /// })
    /// .with_id();  // 启用基于 ID 的优化
    /// ```
    pub fn with_id(self) -> ForEachIdentifiable<T, V>
    where
        T: Identifiable,
        T::Id: Hash + Eq + Send + Sync,
    {
        ForEachIdentifiable {
            data: self.data,
            builder: self.builder,
            spacing: self.spacing,
            padding: self.padding,
            scrollable: self.scrollable,
            scroll_config: self.scroll_config,
            _phantom: PhantomData,
        }
    }
}

/// ForEach 的优化变体 - 使用 Identifiable trait 进行增量更新
///
/// 此结构通过 ID 跟踪列表元素，实现了视图缓存和差异更新。
/// 当数据变化时，只有新增、删除或修改的元素会重建视图。
pub struct ForEachIdentifiable<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: DeclarativeView + 'static,
{
    data: Binding<Vec<T>>,
    builder: Arc<BuilderFn<T, V>>,
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scrollable: Binding<bool>,
    scroll_config: Binding<ScrollConfig>,
    _phantom: PhantomData<V>,
}

impl<T, V> DeclarativeView for ForEachIdentifiable<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: DeclarativeView + 'static,
{
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(EmptyView)
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let padding = self.padding.get();
        let spacing = self.spacing.get();

        let content_area = apply_padding(area, padding);
        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        let children = self.build_children();

        let mut y = content_area.y;
        let bottom = content_area.y.saturating_add(content_area.height);

        for (idx, child) in children.iter().enumerate() {
            if y >= bottom {
                break;
            }

            let height_left = bottom.saturating_sub(y);
            let child_area = Rect {
                x: content_area.x,
                y,
                width: content_area.width,
                height: 1.min(height_left),
            };

            child.render(frame, child_area, ctx);

            y = y.saturating_add(child_area.height);
            if idx + 1 < children.len() {
                y = y.saturating_add(spacing);
            }
        }
    }

    fn build_view(&self) -> Box<dyn View> {
        let vstack = VStackView::new()
            .with_padding(self.padding.clone())
            .with_spacing(self.spacing.clone())
            .with_scrollable(self.scrollable.clone())
            .with_scroll_config(self.scroll_config.clone());

        let mut view = ForEachIdentifiableView {
            data: self.data.clone(),
            builder: self.builder.clone(),
            cached_view: Box::new(vstack),
            cached_items: HashMap::new(),
            cached_ids: Vec::new(),
            data_observer: self.data.dirty_observer(),
            _phantom: PhantomData,
        };
        view.reconcile_children();
        Box::new(view)
    }
}

impl<T, V> ForEachIdentifiable<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: DeclarativeView + 'static,
{
    fn build_children(&self) -> Vec<V> {
        let items = self.data.get();
        items
            .iter()
            .enumerate()
            .map(|(idx, item)| (self.builder)(item, idx))
            .collect()
    }
}

impl<T, V> DeclarativeView for ForEach<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: DeclarativeView + 'static,
{
    fn body(&self) -> Box<dyn DeclarativeView> {
        Box::new(EmptyView)
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let padding = self.padding.get();
        let spacing = self.spacing.get();

        let content_area = apply_padding(area, padding);
        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        let children = self.build_children();

        let mut y = content_area.y;
        let bottom = content_area.y.saturating_add(content_area.height);

        for (idx, child) in children.iter().enumerate() {
            if y >= bottom {
                break;
            }

            let height_left = bottom.saturating_sub(y);
            let child_area = Rect {
                x: content_area.x,
                y,
                width: content_area.width,
                height: 1.min(height_left),
            };

            child.render(frame, child_area, ctx);

            y = y.saturating_add(child_area.height);
            if idx + 1 < children.len() {
                y = y.saturating_add(spacing);
            }
        }
    }

    fn build_view(&self) -> Box<dyn View> {
        let vstack = VStackView::new()
            .with_padding(self.padding.clone())
            .with_spacing(self.spacing.clone())
            .with_scrollable(self.scrollable.clone())
            .with_scroll_config(self.scroll_config.clone());

        let mut view = ForEachView {
            data: self.data.clone(),
            builder: self.builder.clone(),
            cached_view: Box::new(vstack),
            data_observer: self.data.dirty_observer(),
            _phantom: PhantomData,
        };
        view.rebuild_children();
        Box::new(view)
    }
}

/// 命令式 ForEach 视图实现
///
/// 这个视图会在每次渲染时检查数据是否变化，如果变化则重新构建内部的 VStackView。
struct ForEachView<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: DeclarativeView + 'static,
{
    data: Binding<Vec<T>>,
    builder: Arc<BuilderFn<T, V>>,
    cached_view: Box<VStackView>,
    data_observer: DirtyObserver,
    _phantom: PhantomData<V>,
}

impl<T, V> ForEachView<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: DeclarativeView + 'static,
{
    fn rebuild_children(&mut self) {
        let items = self.data.get();

        let mut children = Vec::with_capacity(items.len());
        for (idx, item) in items.iter().enumerate() {
            let child_view = (self.builder)(item, idx).build_view();
            children.push(ViewNode::new(child_view).with_layout(default_foreach_item_layout()));
        }

        self.cached_view.replace_children(children);
    }
}

impl<T, V> View for ForEachView<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: DeclarativeView + 'static,
{
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

    fn children(&self) -> &[ViewNode] {
        self.cached_view.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ViewNode>> {
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
        self.cached_view.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.cached_view.set_scroll_offset(x, y);
    }

    fn scroll_to_child(&mut self, child_id: crate::views::ViewId) {
        self.cached_view.scroll_to_child(child_id);
    }

    fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
        ctx: ViewContext<'_>,
    ) -> crate::view::ViewEventResult {
        if self.data.check_dirty(&mut self.data_observer) {
            self.rebuild_children();
        }

        self.cached_view.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if self.data.check_dirty(&mut self.data_observer) {
            self.rebuild_children();
        }

        self.cached_view.draw(frame, area, ctx);
    }

    fn is_focusable(&self) -> bool {
        self.cached_view.is_focusable()
    }
}

/// 基于 Identifiable 的 ForEach 视图实现（支持增量更新）
///
/// 此视图维护一个 ID 到视图的映射缓存，当数据更新时：
/// 1. 对于新增的元素，创建新视图
/// 2. 对于删除的元素，移除缓存的视图
/// 3. 对于保持不变的元素，复用缓存的视图
/// 4. 对于修改的元素（ID 相同但内容变化），重建视图
struct ForEachIdentifiableView<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: DeclarativeView + 'static,
{
    data: Binding<Vec<T>>,
    builder: Arc<BuilderFn<T, V>>,
    cached_view: Box<VStackView>,
    cached_items: HashMap<T::Id, T>,
    cached_ids: Vec<T::Id>,
    data_observer: DirtyObserver,
    _phantom: PhantomData<V>,
}

impl<T, V> ForEachIdentifiableView<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: DeclarativeView + 'static,
{
    fn reconcile_children(&mut self) {
        let items = self.data.get();

        let old_ids = std::mem::take(&mut self.cached_ids);
        let old_children = {
            let children = self
                .cached_view
                .children_mut()
                .expect("VStackView should expose children_mut");
            std::mem::take(children)
        };

        let mut old_by_id: HashMap<T::Id, ViewNode> = HashMap::with_capacity(old_children.len());
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
                    // Ensure the default layout is correct even for views created before this logic existed.
                    node.layout = default_foreach_item_layout();

                    let needs_rebuild = old_cached_items.get(&id) != Some(item);
                    if needs_rebuild {
                        node.view = (self.builder)(item, idx).build_view();
                    }
                    node
                }
                None => {
                    let child_view = (self.builder)(item, idx).build_view();
                    ViewNode::new(child_view).with_layout(default_foreach_item_layout())
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

impl<T, V> View for ForEachIdentifiableView<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: DeclarativeView + 'static,
{
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

    fn children(&self) -> &[ViewNode] {
        self.cached_view.children()
    }

    fn children_mut(&mut self) -> Option<&mut Vec<ViewNode>> {
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
        self.cached_view.scroll_config()
    }

    fn set_scroll_offset(&mut self, x: u16, y: u16) {
        self.cached_view.set_scroll_offset(x, y);
    }

    fn scroll_to_child(&mut self, child_id: crate::views::ViewId) {
        self.cached_view.scroll_to_child(child_id);
    }

    fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
        ctx: ViewContext<'_>,
    ) -> crate::view::ViewEventResult {
        if self.data.check_dirty(&mut self.data_observer) {
            self.reconcile_children();
        }

        self.cached_view.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        if self.data.check_dirty(&mut self.data_observer) {
            self.reconcile_children();
        }

        self.cached_view.draw(frame, area, ctx);
    }

    fn is_focusable(&self) -> bool {
        self.cached_view.is_focusable()
    }
}

fn apply_padding(area: Rect, padding: EdgeInsets) -> Rect {
    let x = area.x.saturating_add(padding.left);
    let y = area.y.saturating_add(padding.top);
    let width = area
        .width
        .saturating_sub(padding.left.saturating_add(padding.right));
    let height = area
        .height
        .saturating_sub(padding.top.saturating_add(padding.bottom));

    Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declarative::Text;
    use crate::reactive::Property;
    use crate::theme::Theme;
    use crate::view::{ScrollbarHost, TabMode};
    use crate::wm::WindowId;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn draw_imperative(view: &mut dyn View, area: Rect, scrollbar_host: ScrollbarHost) {
        let theme = Theme::dark();
        let ctx = ViewContext {
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

        // 验证空列表不会崩溃
        let children = for_each.build_children();
        assert_eq!(children.len(), 0);
    }

    #[test]
    fn test_foreach_simple_list() {
        let data = Property::new(vec!["Apple".to_string(), "Banana".to_string()]);
        let for_each = ForEach::new(data.binding(), |item, _idx| Text::new(item.clone()));

        let children = for_each.build_children();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_foreach_with_index() {
        let data = Property::new(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        let for_each = ForEach::new(data.binding(), |item, idx| {
            Text::new(format!("{idx}. {item}"))
        });

        let children = for_each.build_children();
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn test_foreach_dynamic_update() {
        let data = Property::new(vec!["A".to_string()]);
        let for_each = ForEach::new(data.binding(), |item, _idx| Text::new(item.clone()));

        // 初始状态
        let children = for_each.build_children();
        assert_eq!(children.len(), 1);

        // 添加元素
        data.set(vec!["A".to_string(), "B".to_string()]);

        // 验证子元素数量变化
        let children = for_each.build_children();
        assert_eq!(children.len(), 2);

        // 清空列表
        data.set(Vec::new());
        let children = for_each.build_children();
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

        // 验证可以创建优化的 ForEach
        let children = for_each.build_children();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn foreach_scroll_metrics_reflect_content_rows() {
        let data = Property::new((0..20).map(|i| format!("Item {i}")).collect::<Vec<_>>());
        let for_each = ForEach::new(data.binding(), |item, idx| {
            Text::new(format!("{idx}: {item}"))
        })
        .scrollable(true);

        let mut view = for_each.build_view();
        draw_imperative(&mut *view, Rect::new(0, 0, 20, 5), ScrollbarHost::Window);

        assert!(view.is_scrollable(), "expected ForEach to be scrollable");
        assert!(
            !view.is_focusable(),
            "Text rows are not focusable, so ForEach should not be focusable"
        );

        let (_content_w, content_h) = view.content_size();
        let (_viewport_w, viewport_h) = view.viewport_size();
        assert!(
            content_h > viewport_h,
            "expected content height ({content_h}) to exceed viewport height ({viewport_h})"
        );
    }

    #[test]
    fn foreach_multiple_views_share_binding_without_missing_updates() {
        let data = Property::new(vec!["A".to_string()]);
        let binding = data.binding();

        let for_each_1 =
            ForEach::new(binding.clone(), |item, _| Text::new(item.clone())).scrollable(true);
        let for_each_2 = ForEach::new(binding, |item, _| Text::new(item.clone())).scrollable(true);

        let mut view1 = for_each_1.build_view();
        let mut view2 = for_each_2.build_view();

        draw_imperative(&mut *view1, Rect::new(0, 0, 10, 3), ScrollbarHost::View);
        draw_imperative(&mut *view2, Rect::new(0, 0, 10, 3), ScrollbarHost::View);

        data.set(vec!["A".to_string(), "B".to_string()]);

        // If one view clears a shared dirty flag, the other view would miss the update.
        draw_imperative(&mut *view1, Rect::new(0, 0, 10, 3), ScrollbarHost::View);
        draw_imperative(&mut *view2, Rect::new(0, 0, 10, 3), ScrollbarHost::View);

        assert_eq!(view1.children().len(), 2);
        assert_eq!(view2.children().len(), 2);
    }

    #[test]
    fn foreach_with_id_reuses_child_views_across_draws_and_reorder() {
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

        let build_count = Arc::new(AtomicUsize::new(0));
        let build_count_for_builder = Arc::clone(&build_count);

        let data = Property::new(vec![
            Item {
                id: 1,
                value: "A".to_string(),
            },
            Item {
                id: 2,
                value: "B".to_string(),
            },
            Item {
                id: 3,
                value: "C".to_string(),
            },
        ]);

        let list = ForEach::new(data.binding(), move |item, _idx| {
            build_count_for_builder.fetch_add(1, Ordering::Relaxed);
            Text::new(item.value.clone())
        })
        .scrollable(true)
        .with_id();

        let mut view = list.build_view();
        assert_eq!(
            build_count.load(Ordering::Relaxed),
            3,
            "initial build should create one view per item"
        );

        draw_imperative(&mut *view, Rect::new(0, 0, 10, 3), ScrollbarHost::View);
        draw_imperative(&mut *view, Rect::new(0, 0, 10, 3), ScrollbarHost::View);

        assert_eq!(
            build_count.load(Ordering::Relaxed),
            3,
            "drawing without data changes should not rebuild children"
        );

        let before_ids: Vec<_> = view.children().iter().map(|c| c.id).collect();

        // Reorder items (same IDs, same content).
        data.set(vec![
            Item {
                id: 3,
                value: "C".to_string(),
            },
            Item {
                id: 1,
                value: "A".to_string(),
            },
            Item {
                id: 2,
                value: "B".to_string(),
            },
        ]);
        draw_imperative(&mut *view, Rect::new(0, 0, 10, 3), ScrollbarHost::View);

        assert_eq!(
            build_count.load(Ordering::Relaxed),
            3,
            "reordering should reuse existing child views"
        );

        let after_ids: Vec<_> = view.children().iter().map(|c| c.id).collect();
        assert_ne!(before_ids, after_ids, "expected child order to change");

        let mut before_sorted = before_ids.clone();
        before_sorted.sort_by_key(|id| id.0);
        let mut after_sorted = after_ids.clone();
        after_sorted.sort_by_key(|id| id.0);
        assert_eq!(
            before_sorted, after_sorted,
            "expected child view identities to be preserved across reorder"
        );
    }

    #[test]
    fn foreach_with_id_rebuilds_only_changed_items() {
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

        let build_count = Arc::new(AtomicUsize::new(0));
        let build_count_for_builder = Arc::clone(&build_count);

        let data = Property::new(vec![
            Item {
                id: 1,
                value: "A".to_string(),
            },
            Item {
                id: 2,
                value: "B".to_string(),
            },
            Item {
                id: 3,
                value: "C".to_string(),
            },
        ]);

        let list = ForEach::new(data.binding(), move |item, _idx| {
            build_count_for_builder.fetch_add(1, Ordering::Relaxed);
            Text::new(item.value.clone())
        })
        .with_id();

        let mut view = list.build_view();
        assert_eq!(build_count.load(Ordering::Relaxed), 3);

        // Change only one item.
        data.set(vec![
            Item {
                id: 1,
                value: "A".to_string(),
            },
            Item {
                id: 2,
                value: "B (changed)".to_string(),
            },
            Item {
                id: 3,
                value: "C".to_string(),
            },
        ]);

        draw_imperative(&mut *view, Rect::new(0, 0, 10, 3), ScrollbarHost::View);

        assert_eq!(
            build_count.load(Ordering::Relaxed),
            4,
            "expected only the changed item to be rebuilt"
        );
    }
}
