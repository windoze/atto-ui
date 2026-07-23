# 动态组件树：单一真值改造方案（消除 view/spec 双真值）

状态：已实施（reconcile 方案，B4 症状1 根治 / 症状2 以契约固化）
关联问题：`DESIGN_REVIEW.md` B4
涉及文件：`src/runtime/tree.rs`（核心）、`src/composable/component.rs` 与 `src/runtime/tree.rs` 的 doc 契约

---

## 0. 修订记录：从"删除 root（派生 to_spec）"改为"reconcile（惰性同步镜像）"

初版方案主张 view 单真值、删除 `root` 字段、spec 由 `view.to_spec()` 现场派生。落地前核实反射面时发现三个硬阻塞，导致该路径成本高且核心验收标准不成立：

1. **无 type_name 反向映射**：`ComponentSpec.type_name` 是注册名（`"Checkbox"`），而 `Component::type_name()` 用默认 `std::any::type_name`（Rust 全路径），无组件 override，registry 也无反查。`to_spec` 拼不出 type_name。
2. **事件绑定读不回**：build 时 `CallbackId` 被埋进私有的 `CallbackHandle`，Component trait 无读回事件绑定的方法。`to_spec` 重建不了 `events`。
3. **默认值物化 → round-trip 恒不相等**：builtin 构造用 `prop_bool(spec,"enabled")?.unwrap_or(true)`，spec 省略的键 build 后被物化，`to_spec` 读 `get_property` 必然写回，`build(spec).to_spec() == spec`（原定核心验收/安全网）对任何依赖默认值的真实 spec 都不成立，且无解（除非改宏为每个反应式字段加"显式设置"追踪，波及全项目）。

**关键收敛观察**：B4 实际丢失的永远是 **props（可变状态：文本、勾选、光标）**，而 props 恰好是唯一可经现有 `get_property` 从 view 读回的部分；**结构（type_name/events/layout/meta）只由 tree-ops 修改，而 tree-ops 已同步 root**，用户交互/`apply_command` 从不触碰结构。因此硬啃方案为"重建结构"付出的额外成本（type_name 反向映射 + 事件反射 + 碰全部 builtins）对修 B4 **零收益**——处理 props 的机制两方案完全相同（都靠 `get_property`）。

**决定**：改走 reconcile。保留 `root` 作**结构骨架**，把 props 的真值权收敛到 view：在会丢状态的时机（rebuild 前）和会读到陈旧值的时机（dynamic_root_spec 读前）用 `get_property` 把 view 的 props 同步回 root。语义上达成"view 是 props 唯一权威、root 是它的结构镜像"，B4 两个症状都根治，改动仅限 `src/runtime/tree.rs`。物理上 `root` 字段不删除（不追求"单一结构体字段"，追求"单一真值语义"）。

---

## 1. 问题回顾

`ComponentTree`（`src/runtime/tree.rs:20`）同时持有同一棵 UI 树的两份表示：

```rust
pub struct ComponentTree {
    root: ComponentSpec,        // 声明式蓝图（可序列化数据）
    view: Box<dyn Component>,   // 活实例（含 Binding、运行时状态）
    callbacks: CallbackRegistry,
    registry: ComponentRegistry<Box<dyn Component>>,
}
```

两条写路径**不对称**：

| 写路径 | 触发者 | 改 view | 改 root |
|---|---|---|---|
| `apply_ops_incremental` | 外部 tree-ops（React reconciler 等） | ✅ | ✅（成功后 `self.root = next_root`） |
| `set_property` / `apply_command` | 用户交互、脚本 | ✅ | ❌ 从不回写 |

后果（两个真实缺陷）：

1. **回退式 rebuild 静默吞输入**：`apply_ops_incremental` 中任一步失败或 `view_shape_matches_spec` 为假时，走 `rebuild_next_or_restore` → `replace_with_rebuilt_root` → `self.view = registry.build(&root)`，即从**陈旧的 root** 重建 view，用户在控件里的输入（只存在于旧 view）被丢弃。
2. **两个读接口给矛盾答案**：`get_property`（`tree.rs:388`）读 view，`dynamic_root_spec()` / `root_spec()`（`tree.rs:514`）读 root。用户改过属性后，二者对同一属性返回不同值；introspection / 跨语言绑定基于陈旧 root 判断。

---

## 2. 决策：view 为唯一真值，spec 降级为派生投影

**依据（从"被 Rust 程序消费"的角度）——这是硬事实，不是审美选择：**

`ComponentTree` 的 `impl Component`（`tree.rs:376` 起）是 Rust 渲染/事件循环真正调用的入口，**每一个 trait 方法都转发给 `self.view`，无一读 `self.root`**：`draw` / `handle_event` / `get_property` / `set_property` / `apply_command` / `children` / `focus` 全部走 view。运行时热路径**只消费 view**；`root` 唯一的实质作用是 `apply_ops_incremental` 里当**回滚基线**与形状校验。

三个结构性理由：

1. **view 是唯一完整状态源**：光标、滚动、焦点、`Binding` 值只在 view 里；`ComponentSpec` 只有 `type_name/id/props/events/children`，装不下运行时状态。
2. **spec 对 Rust 是外来序列化格式**：`BTreeMap<String, ComponentValue>` 是为跨语言而生，Rust 消费它要 `prop_string("text")` 查表 + 解码；view 是静态类型组件，直接拿 `&str`。热路径消费 spec = 自愿放弃类型系统。
3. **现状已默认 view 是真值**：所谓"双真值"实为 view 单真值 + 一个没跟上的陈旧 spec 副本。本方案是把架构已在做的事**正式化**。

**spec 的正确定位**：出向序列化投影，服务非 Rust 消费者（Python/Node 绑定、React reconciler、introspection 快照）。需要时由 `view.to_spec()` 现场生成，只在跨语言/序列化边界物化。

目标形态（reconcile 落地版："单一真值语义 + 惰性同步镜像"）：

```
          ┌─────────────── ComponentTree ───────────────┐
入向 ────► │  apply_ops(patch)  ── 改结构，同步 root       │
(tree-ops) │       │                                      │
交互 ─────►│  set_property/apply_command ── 只改 view      │
           │       ▼                                      │
           │  view: Box<dyn Component>  ← props 唯一权威   │
           │       │  reconcile_spec_props (rebuild 前)   │
           │       ▼                                      │
镜像 ◄──── │  root: ComponentSpec  ← 结构骨架 + props 镜像 │
           └──────────────────────────────────────────────┘
```

`root: ComponentSpec` 字段**保留**（作结构骨架 + props 惰性镜像），不删除。语义上 view 是 props 的唯一权威。

### 为什么不选 spec 当真值（方案 B）

spec 当真值 = React controlled-component 模型，用户输入不许停在 view、`apply_command` 必须回写 spec 再 patch view。这与整个 widget 体系基于 `Binding`、自持状态的设计**正面冲突**，改动面远大于收益。除非整体倒向纯声明式，否则不采纳。

---

## 3. 落地实现（as-built）

改动仅限 `src/runtime/tree.rs`（核心）+ 两处 doc（`component.rs`、`tree.rs`）。

### 3.1 核心：`reconcile_spec_props`

按 `tag` 让 spec 与 view 在层级上对齐，用现有 `get_property` 把 view 当前 props 刷回 spec：

```rust
fn reconcile_spec_props(spec: &mut ComponentSpec, view: &dyn Component) {
    if spec.id.as_deref() != view.tag() {
        return;
    }
    for (name, value) in spec.props.iter_mut() {
        if let Some(current) = view.get_property(name) {
            *value = current;               // 只刷新已声明的键
        }
    }
    let children = view.children();
    if spec.children.len() != children.len() {
        return;                             // 形状不符则跳过该子树，交给 view_shape_matches_spec
    }
    for (child_spec, child_view) in spec.children.iter_mut().zip(children.iter()) {
        reconcile_spec_props(child_spec.node.as_mut(), child_view.view.as_ref());
    }
}
```

关键设计点：

- **只刷新 `spec.props` 里已存在的键，绝不新增**。这就绕开了"默认值物化"陷阱——spec 省略的键（如 `enabled`）不会被 `get_property` 读到的默认值污染，spec 保持为宿主声明的忠实镜像。
- **`get_property` 返回 `None` 的键保持不动**，不删除。
- **形状不符即跳过**该子树的 reconcile，形状一致性仍由既有 `view_shape_matches_spec` 负责，职责不重叠。

### 3.2 调用点（症状1：rebuild 吞输入）

`ComponentTree::sync_root_from_view()` 封装 reconcile，在所有"从 root 重建 view"之前调用：

- `apply_ops_incremental` 入口（clone `original_root`/`next_root` **之前**，使两个快照都带上最新 props；随后 ops 应用在其上，故入向 ops 仍然覆盖 reconcile 的值——ops 优先）。
- `apply_ops_and_rebuild` 入口。
- `rebuild()` 入口。

效果：任何回退式 rebuild 前，view 的用户输入已回写 root，不再被陈旧 root 覆盖丢弃。

### 3.3 症状2（两读接口对 props 值不一致）—— 核实后不改代码，仅明确契约

落地阶段核实发现：**生产代码里没有从 `dynamic_root_spec` 读取 props 值的消费者**。

- inspect 快照（Python/Node 消费的 `DesktopSnapshot`）的 `properties` 由 `view.get_property()` 组装（`inspect.rs:component_snapshot_fields`），总是新鲜。
- `query` / 绑定的 `get_property` 直接读 view。
- 唯一从 `dynamic_root_spec().props` 读值的是一个 wm 测试（`wm/manager/tests.rs`），且因 tree-ops 已同步 root 而正确。
- 其余 `dynamic_root_spec` 调用点（`tab_window`、`desktop`、`wm/window`、`border`、`visibility`…）都只做**结构/存在性**检查，不读 props 值。

而把 `dynamic_root_spec(&self) -> Option<&ComponentSpec>` 改成能 reconcile（需 `&mut self` 或内部可变），要动 ~11 个调用点 + 两个 trait 默认实现，且会危及那些真正在用的借用式结构读取。**投入大、生产收益近乎为零**。

因此症状2 采取"明确契约"而非改代码：给 `DynamicTree::dynamic_root_spec`（`component.rs`）和 `ComponentTree::root_spec`（`tree.rs`）加 doc，声明它是**结构骨架快照**，其 `props` 值在两次 reconcile 之间可能滞后；**读当前属性值一律用 `Component::get_property`（读 view）**。这与现有生产代码路径一致，把隐性约定固化为显式契约。

### 3.4 未纳入本次改造

- 绑定回调存活性（`DESIGN_REVIEW.md` M1）：独立问题，不在 B4 范围。
- inspect 快照裁剪信息丢失（inspect M1）：独立问题。

---

## 4. 影响与验证

**收益**
- 症状1（rebuild 吞输入）从根上消除：view 成为 props 的唯一权威，rebuild 前必先 reconcile。
- 症状2（读接口矛盾）：生产路径本就统一走 view，现以 doc 契约固化，杜绝未来误用 `dynamic_root_spec` 读 props 值。
- 改动面极小（一个文件 + 两处 doc），无 builtin 改动，无 API 破坏。

**验证**
- 新增 `component_tree_reconciles_view_edits_into_root_before_rebuild`：仅在 view 上编辑，触发回退式 rebuild，断言编辑存活；**临时禁用 reconcile 时该测试失败**（left=`"old"`, right=`"typed"`），证明它真的守护该行为。
- 既有 42 个 runtime 测试（含 root 回滚、默认值 rebuild、增量 ops 全套）全绿；全 413 个 lib 测试全绿；clippy 干净。

**残留（有意未做）**
- `root` 字段物理保留（本方案只追求 props 单一真值语义，不追求删字段）。
- 结构信息（type_name/events/layout/meta）仍以 root 为准——它们只由 tree-ops 改且已同步，无双真值问题。
