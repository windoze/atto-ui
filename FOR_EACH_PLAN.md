# ForEach 容器实现计划

## 概述

ForEach 是一个声明式容器组件，用于根据数据列表动态生成子视图。它解决了以下核心问题：

1. **动态数据源绑定** - 数据列表变化时自动重建子元素
2. **元素唯一标识** - 支持高效的差异更新（类似 SwiftUI 的 Identifiable）
3. **子元素状态绑定** - 每个元素可以有自己的独立状态
4. **回调传递** - 为每个元素传递上下文（索引、ID、数据）

## 三阶段实现计划

### 第一阶段：MVP (Minimum Viable Product)

**目标**: 实现基础的 ForEach 功能，能够根据 `Binding<Vec<T>>` 动态生成子视图。

**实现内容**:

1. **核心结构** (`src/declarative/for_each.rs`)
   ```rust
   pub struct ForEach<T, V>
   where
       T: Clone + Send + 'static,
       V: DeclarativeView + 'static,
   {
       data: Binding<Vec<T>>,
       builder: Arc<dyn Fn(&T, usize) -> V + Send + Sync>,
       spacing: Option<u16>,
       _phantom: PhantomData<V>,
   }
   ```

2. **构造器和构建器方法**
   - `new(data: Binding<Vec<T>>, builder: F)` - 创建 ForEach 实例
   - `spacing(spacing: u16)` - 设置子元素间距
   - `padding(padding: impl Into<Binding<EdgeInsets>>)` - 设置内边距（可选）

3. **DeclarativeView 实现**
   - 在 `body()` 方法中动态构建 `VStack`
   - 遍历 `data.get()` 获取当前数据
   - 为每个元素调用 `builder` 生成子视图
   - 利用 Binding 的脏标记机制自动触发重渲染

4. **模块导出**
   - 在 `src/declarative/mod.rs` 中添加 `pub mod for_each;`
   - 导出 `pub use for_each::ForEach;`

**测试内容**:

1. 简单列表渲染（静态数据）
2. 动态添加/删除元素
3. 空列表处理
4. 索引传递验证

**成功标准**:
- ✅ ForEach 能够渲染简单的文本列表
- ✅ 数据变化时视图自动更新
- ✅ spacing 参数生效
- ✅ 通过所有 MVP 测试

---

### 第二阶段：状态和回调支持

**目标**: 支持复杂的交互场景，包括独立的元素状态和事件回调。

**实现内容**:

1. **文档和示例**
   - 编写"数据自带状态"模式的文档
   - 提供 TodoList 示例（带 Checkbox 状态）
   - 提供按钮列表示例（带点击回调）

2. **高级构建器方法**
   - `horizontal()` - 使用 HStack 而非 VStack
   - `grid(columns: usize)` - 使用 Grid 布局（可选）
   - `alignment(align: Align)` - 设置对齐方式

3. **回调模式文档化**
   - 闭包捕获模式（Arc 克隆）
   - 外部状态映射模式
   - 事件冒泡模式（如果需要）

4. **示例应用** (`examples/foreach_demo.rs`)
   - 待办事项列表（每个元素有 Checkbox 状态）
   - 用户列表（带点击回调）
   - 动态添加/删除元素的交互

**测试内容**:

1. 带独立状态的元素列表
2. 回调函数正确传递元素信息
3. 复杂嵌套场景（ForEach 内部嵌套其他容器）
4. HStack 和 Grid 布局模式

**成功标准**:
- ✅ 支持元素独立状态绑定
- ✅ 回调能正确捕获元素上下文
- ✅ 提供完整的示例应用
- ✅ 文档清晰易懂

---

### 第三阶段：性能优化

**目标**: 支持大规模数据列表的高效渲染和更新。

**实现内容**:

1. **Identifiable Trait** (`src/declarative/identifiable.rs`)
   ```rust
   pub trait Identifiable {
       type Id: Eq + Hash + Clone;
       fn id(&self) -> Self::Id;
   }
   ```

2. **视图缓存机制**
   - 在 ForEach 中添加 `cache: HashMap<T::Id, Box<dyn View>>`
   - 实现基于 ID 的差异更新算法
   - 只重建发生变化的元素

3. **增量更新优化**
   - 检测插入、删除、移动操作
   - 最小化重建开销
   - 保持元素状态（焦点、滚动位置等）

4. **虚拟滚动集成**
   - 与 `ScrollView` 的虚拟滚动机制集成
   - 只渲染可见区域的元素
   - 支持数千甚至数万条数据的流畅滚动

5. **性能基准测试**
   - 测试不同数据规模下的渲染性能
   - 对比缓存前后的性能差异
   - 优化热点路径

**测试内容**:

1. 1000+ 元素的大列表渲染
2. 频繁增删改操作的性能
3. 虚拟滚动的正确性
4. 缓存命中率验证
5. 内存占用测试

**成功标准**:
- ✅ 支持 Identifiable 数据源
- ✅ 大列表渲染流畅（60 FPS）
- ✅ 增量更新减少 70% 以上重建开销
- ✅ 虚拟滚动正常工作
- ✅ 通过所有性能测试

---

## 设计决策

### 1. 子元素状态绑定模式

**推荐模式：数据自带状态**

数据结构本身包含 `Property` 字段，ForEach 直接绑定到这些属性。

```rust
struct TodoItem {
    id: usize,
    text: String,
    completed: Property<bool>,  // 状态存储在数据中
}

let todos = Property::new(vec![
    TodoItem::new(1, "Task 1"),
    TodoItem::new(2, "Task 2"),
]);

ForEach::new(todos.binding(), |todo, _| {
    HStack::new()
        .child(Checkbox::new("", todo.completed.binding()))  // 直接绑定
        .child(Text::new(&todo.text))
})
```

**优点**:
- 类型安全
- 状态与数据紧密耦合
- 易于理解和维护

### 2. 回调传递策略

**推荐策略：闭包捕获**

使用 Arc 克隆和闭包捕获传递元素上下文到回调函数。

```rust
let on_delete = Arc::new(|todo_id: usize| {
    println!("Delete todo {todo_id}");
});

ForEach::new(todos.binding(), move |todo, _| {
    let on_delete = on_delete.clone();
    let todo_id = todo.id;

    HStack::new()
        .child(Text::new(&todo.text))
        .child(Button::new("Delete").on_click(move || {
            on_delete(todo_id);  // 捕获具体元素的 ID
        }))
})
```

**优点**:
- 类型安全
- 利用 Rust 的所有权系统
- 符合现有 Button::on_click 的 API

### 3. 布局容器选择

ForEach 默认使用 `VStack` 作为内部容器，但提供方法切换到其他布局：

- `ForEach::new().vertical()` - VStack（默认）
- `ForEach::new().horizontal()` - HStack
- `ForEach::new().grid(columns)` - Grid

### 4. 性能优化权衡

**第一阶段**: 简单重建，无缓存
- 优点：实现简单，代码清晰
- 缺点：大列表性能差

**第三阶段**: 基于 ID 的缓存和差异更新
- 优点：高性能，支持大列表
- 缺点：实现复杂，需要 Identifiable trait

**策略**: 对于小列表（< 100 元素），简单重建即可；大列表必须使用缓存和虚拟滚动。

---

## 实现检查清单

### 第一阶段（MVP）
- [ ] 创建 `src/declarative/for_each.rs`
- [ ] 实现 `ForEach<T, V>` 结构
- [ ] 实现 `new()` 和 `spacing()` 方法
- [ ] 实现 `DeclarativeView` trait
- [ ] 在 `mod.rs` 中导出
- [ ] 编写基础测试（`tests/pty_foreach.rs`）
- [ ] 创建简单演示应用

### 第二阶段（状态和回调）
- [ ] 编写状态绑定文档
- [ ] 编写回调模式文档
- [ ] 实现 `horizontal()` 方法
- [ ] 实现 `alignment()` 和其他布局参数
- [ ] 创建 TodoList 示例
- [ ] 创建用户列表示例
- [ ] 编写复杂交互测试

### 第三阶段（性能优化）
- [ ] 创建 `src/declarative/identifiable.rs`
- [ ] 实现 `Identifiable` trait
- [ ] 实现视图缓存机制
- [ ] 实现差异更新算法
- [ ] 集成虚拟滚动
- [ ] 编写性能基准测试
- [ ] 优化热点路径
- [ ] 内存占用分析

---

## 测试策略

### 单元测试

在 `src/declarative/for_each.rs` 中添加模块内测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foreach_empty_list() {
        let data = Property::new(Vec::<String>::new());
        let for_each = ForEach::new(data.binding(), |item, _| {
            Text::new(item)
        });
        // 验证空列表不会崩溃
    }

    #[test]
    fn test_foreach_dynamic_update() {
        let data = Property::new(vec!["A".to_string()]);
        let for_each = ForEach::new(data.binding(), |item, _| {
            Text::new(item)
        });

        // 添加元素
        data.set(vec!["A".to_string(), "B".to_string()]);

        // 验证子元素数量变化
    }
}
```

### 集成测试（PTY）

创建 `tests/pty_foreach.rs`：

```rust
use chatty_test_host::PtyTestHost;
use std::time::Duration;

#[test]
fn test_foreach_renders_list() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_foreach_demo");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    host.wait_for_text("1. Apple", Duration::from_secs(2))?;
    host.wait_for_text("2. Banana", Duration::from_secs(2))?;
    host.wait_for_text("3. Cherry", Duration::from_secs(2))?;

    Ok(())
}

#[test]
fn test_foreach_dynamic_add() -> anyhow::Result<()> {
    let bin = env!("CARGO_BIN_EXE_foreach_demo");
    let mut host = PtyTestHost::spawn(bin, &[], 80, 24)?;

    // 按键添加元素
    host.send_key(KeyCode::Char('a'))?;

    host.wait_for_text("4. Durian", Duration::from_secs(2))?;

    Ok(())
}
```

### 性能测试

创建 `benches/foreach_benchmark.rs`（第三阶段）：

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_foreach_render(c: &mut Criterion) {
    c.bench_function("foreach 1000 items", |b| {
        b.iter(|| {
            // 渲染 1000 个元素的 ForEach
        });
    });
}

criterion_group!(benches, bench_foreach_render);
criterion_main!(benches);
```

---

## 文档计划

### API 文档

在 `src/declarative/for_each.rs` 中添加详细的 rustdoc：

```rust
/// ForEach 容器 - 根据数据列表动态生成子视图
///
/// ForEach 是声明式 API 的核心容器之一，用于高效地渲染数据列表。
/// 它会根据绑定的数据源自动创建、更新和删除子视图。
///
/// # 基础用法
///
/// ```rust
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
/// ```rust
/// struct TodoItem {
///     text: String,
///     completed: Property<bool>,
/// }
///
/// let todos = Property::new(vec![/* ... */]);
///
/// ForEach::new(todos.binding(), |todo, _| {
///     HStack::new()
///         .child(Checkbox::new("", todo.completed.binding()))
///         .child(Text::new(&todo.text))
/// })
/// ```
///
/// # 回调处理
///
/// 使用闭包捕获传递元素上下文到回调：
///
/// ```rust
/// let on_click = Arc::new(|id: usize| println!("Clicked {id}"));
///
/// ForEach::new(items.binding(), move |item, _| {
///     let on_click = on_click.clone();
///     let item_id = item.id;
///
///     Button::new(&item.name)
///         .on_click(move || on_click(item_id))
/// })
/// ```
///
/// # 性能
///
/// 对于大列表（>100 元素），建议使用 `Identifiable` trait 启用缓存优化：
///
/// ```rust
/// impl Identifiable for MyItem {
///     type Id = usize;
///     fn id(&self) -> Self::Id {
///         self.id
///     }
/// }
/// ```
pub struct ForEach<T, V> { /* ... */ }
```

### 用户指南

在项目 README 或 CLAUDE.md 中添加 ForEach 使用指南。

---

## 参考资源

### SwiftUI ForEach
- [SwiftUI ForEach Documentation](https://developer.apple.com/documentation/swiftui/foreach)
- Identifiable protocol
- Dynamic list updates

### Jetpack Compose LazyColumn
- [Compose Lists Documentation](https://developer.android.com/jetpack/compose/lists)
- Key-based recomposition
- LazyListState

### 现有 Chatty 组件
- `src/declarative/vstack.rs` - VStack 实现参考
- `src/reactive/property.rs` - Property/Binding 机制
- `src/views/scroll_view.rs` - 虚拟滚动参考

---

## 时间估算（仅供参考）

- **第一阶段**: 基础实现 + 测试
- **第二阶段**: 文档 + 示例 + 高级功能
- **第三阶段**: 性能优化 + 缓存 + 虚拟滚动

每个阶段应该**独立可用**，可以逐步迭代而不影响现有功能。

---

## 风险和挑战

### 技术挑战

1. **DeclarativeView trait object 的克隆问题**
   - 解决方案：在 `body()` 中重新构建，而不是克隆

2. **缓存失效策略**
   - 需要精确的差异算法，避免过度缓存或缓存未命中

3. **虚拟滚动集成复杂度**
   - 需要与现有 ScrollView/ScrollContent 协议配合

### 兼容性

1. 保持与现有声明式 API 的一致性
2. 不破坏现有 VStack/HStack/Grid 的行为
3. 反应式系统的脏标记机制需要正确触发

---

## 成功指标

### 功能指标
- ✅ 支持所有基础数据类型（String, i32, 自定义结构体等）
- ✅ 动态添加/删除/修改元素正常工作
- ✅ 状态绑定和回调传递正确
- ✅ 支持嵌套 ForEach

### 性能指标（第三阶段）
- ✅ 100 元素列表：< 16ms 渲染时间（60 FPS）
- ✅ 1000 元素列表（虚拟滚动）：< 16ms
- ✅ 增量更新：减少 70% 以上重建开销

### 质量指标
- ✅ 100% 测试覆盖率
- ✅ 无 clippy 警告
- ✅ 完整的 API 文档
- ✅ 至少 2 个完整示例

---

## 下一步行动

1. ✅ 创建本计划文档
2. 🔄 实现第一阶段（MVP）
3. ⏳ 评估第一阶段，决定是否继续第二阶段
4. ⏳ 实现第二阶段和第三阶段
5. ⏳ 更新 IMPLEMENTATION_PLAN.md，添加 ForEach 相关里程碑
