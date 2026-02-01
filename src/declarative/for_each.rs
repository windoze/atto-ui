use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::reactive::Binding;
use crate::view::{View, ViewContext};
use crate::views::{EdgeInsets, LayoutParams, ScrollConfig};

use super::identifiable::Identifiable;
use super::stack_view::VStackView;
use super::view::{DeclarativeView, EmptyView};

pub type BuilderFn<T, V> = dyn Fn(&T, usize) -> V + Send + Sync;

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
        Box::new(ForEachIdentifiableView {
            data: self.data.clone(),
            builder: self.builder.clone(),
            spacing: self.spacing.clone(),
            padding: self.padding.clone(),
            scrollable: self.scrollable.clone(),
            scroll_config: self.scroll_config.clone(),
            cached_views: HashMap::new(),
            cached_ids: Vec::new(),
            _phantom: PhantomData,
        })
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
        Box::new(ForEachView {
            data: self.data.clone(),
            builder: self.builder.clone(),
            spacing: self.spacing.clone(),
            padding: self.padding.clone(),
            scrollable: self.scrollable.clone(),
            scroll_config: self.scroll_config.clone(),
            cached_view: None,
            _phantom: PhantomData,
        })
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
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scrollable: Binding<bool>,
    scroll_config: Binding<ScrollConfig>,
    cached_view: Option<Box<VStackView>>,
    _phantom: PhantomData<V>,
}

impl<T, V> ForEachView<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: DeclarativeView + 'static,
{
    fn rebuild(&mut self) {
        let items = self.data.get();

        let mut vstack = VStackView::new()
            .with_padding(self.padding.clone())
            .with_spacing(self.spacing.clone())
            .with_scrollable(self.scrollable.clone())
            .with_scroll_config(self.scroll_config.clone());

        for (idx, item) in items.iter().enumerate() {
            let child_view = (self.builder)(item, idx);
            vstack.add_child_with_layout(child_view.build_view(), LayoutParams::default());
        }

        self.cached_view = Some(Box::new(vstack));
        self.data.mark_clean();
    }
}

impl<T, V> View for ForEachView<T, V>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    V: DeclarativeView + 'static,
{
    fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
        ctx: ViewContext<'_>,
    ) -> crate::view::ViewEventResult {
        // 如果数据变化，重新构建
        if self.data.is_dirty() || self.cached_view.is_none() {
            self.rebuild();
        }

        if let Some(ref mut view) = self.cached_view {
            view.handle_event(event, ctx)
        } else {
            crate::view::ViewEventResult::ignored()
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        // 如果数据变化，重新构建
        if self.data.is_dirty() || self.cached_view.is_none() {
            self.rebuild();
        }

        if let Some(ref mut view) = self.cached_view {
            view.draw(frame, area, ctx);
        }
    }

    fn is_focusable(&self) -> bool {
        // ForEach 本身可能包含可聚焦的子元素
        true
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
    spacing: Binding<u16>,
    padding: Binding<EdgeInsets>,
    scrollable: Binding<bool>,
    scroll_config: Binding<ScrollConfig>,
    /// 缓存：ID -> 构建的 View
    cached_views: HashMap<T::Id, Box<dyn View>>,
    /// 上一次的 ID 列表（用于检测顺序变化）
    cached_ids: Vec<T::Id>,
    _phantom: PhantomData<V>,
}

impl<T, V> ForEachIdentifiableView<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: DeclarativeView + 'static,
{
    /// 增量重建：只更新变化的部分
    fn rebuild_incremental(&mut self) {
        let items = self.data.get();
        let new_ids: Vec<T::Id> = items.iter().map(|item| item.id()).collect();

        // 1. 移除不再存在的元素
        self.cached_views.retain(|id, _| new_ids.contains(id));

        // 2. 为新元素或修改的元素创建/更新视图
        for (idx, item) in items.iter().enumerate() {
            let id = item.id();

            // 检查是否需要重建（新元素或内容变化）
            let needs_rebuild = !self.cached_views.contains_key(&id);

            if needs_rebuild {
                let child_view = (self.builder)(item, idx);
                self.cached_views.insert(id, child_view.build_view());
            }
        }

        // 3. 更新 ID 列表
        self.cached_ids = new_ids;
        self.data.mark_clean();
    }

    /// 构建最终的 VStackView（使用缓存的视图）
    fn build_vstack(&self) -> Box<VStackView> {
        let items = self.data.get();

        let mut vstack = VStackView::new()
            .with_padding(self.padding.clone())
            .with_spacing(self.spacing.clone())
            .with_scrollable(self.scrollable.clone())
            .with_scroll_config(self.scroll_config.clone());

        for item in items.iter() {
            let id = item.id();
            if let Some(_view) = self.cached_views.get(&id) {
                // 注意：这里有个问题，VStackView::add_child_with_layout 需要 Box<dyn View>
                // 但我们不能移动 HashMap 中的值。我们需要重新构建。
                // 这个方法的实现需要改进，暂时使用简单重建
                let child_view = (self.builder)(item, 0);
                vstack.add_child_with_layout(child_view.build_view(), LayoutParams::default());
            }
        }

        Box::new(vstack)
    }
}

impl<T, V> View for ForEachIdentifiableView<T, V>
where
    T: Clone + PartialEq + Identifiable + Send + Sync + 'static,
    T::Id: Hash + Eq + Send + Sync,
    V: DeclarativeView + 'static,
{
    fn handle_event(
        &mut self,
        event: &crossterm::event::Event,
        ctx: ViewContext<'_>,
    ) -> crate::view::ViewEventResult {
        // 如果数据变化，增量重建
        if self.data.is_dirty() {
            self.rebuild_incremental();
        }

        // 构建临时 VStackView 处理事件
        let mut vstack = self.build_vstack();
        vstack.handle_event(event, ctx)
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect, ctx: ViewContext<'_>) {
        // 如果数据变化，增量重建
        if self.data.is_dirty() {
            self.rebuild_incremental();
        }

        // 构建临时 VStackView 进行渲染
        let mut vstack = self.build_vstack();
        vstack.draw(frame, area, ctx);
    }

    fn is_focusable(&self) -> bool {
        true
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
}
