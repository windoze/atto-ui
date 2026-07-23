# 动态组件树：单一真值改造方案（消除 view/spec 双真值）

状态：实施中（采用 reconcile 方案）
关联问题：`DESIGN_REVIEW.md` B4
涉及文件：`src/runtime/tree.rs`（唯一改动点）

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

目标形态（呼应"单一结构 + 两接口"）：

```
         ┌─────────────── ComponentTree ───────────────┐
入向 ───► │  apply_ops(patch)                            │
(tree-ops)│      ↓ 直接改                                │
          │  view: Box<dyn Component>  ← 唯一真值        │
          │      ↑ 派生                                  │
出向 ◄─── │  to_spec() -> ComponentSpec                  │
(序列化)  └──────────────────────────────────────────────┘
```

`root: ComponentSpec` 字段被移除。

### 为什么不选 spec 当真值（方案 B）

spec 当真值 = React controlled-component 模型，用户输入不许停在 view、`apply_command` 必须回写 spec 再 patch view。这与整个 widget 体系基于 `Binding`、自持状态的设计**正面冲突**，改动面远大于收益。除非整体倒向纯声明式，否则不采纳。

---

## 3. 落地清单

### 3.1 前置：让反射面无损（最关键，其余步骤都依赖它）

`to_spec()` 要能无损重建 `ComponentSpec` 的全部字段，但当前反射面有缺口：

| ComponentSpec 字段 | 现有反射能力 | 缺口 |
|---|---|---|
| `type_name` | `Component::type_name()` | ✅ 有 |
| `id` | `Component::tag()` | ✅ 有 |
| `props` | `property_names()` + `get_property()` | ⚠️ 依赖组件如实列全部 prop（需审计各 builtin） |
| `events` | 无 | ❌ 无法读回 `BindEvent` 绑定的 `CallbackId` |
| `children` | `children()` → `ComponentNode` | ⚠️ 有子节点，但下面两项随子附加数据丢失 |
| `children[].layout` | 无统一 getter | ❌ `ComponentSpecChild.layout` 读不回 |
| `children[].meta` | 无 | ❌ `ComponentSpecChild.meta` 读不回 |

佐证：`view_shape_matches_spec`（`tree.rs:600`）目前只能比对 `id` 和 `children.len()`，正说明反射面还不足以做无损往返。

**要做的事：**
1. 在 `Component`（或 `DynamicTree`）trait 上新增：
   - `fn event_bindings(&self) -> BTreeMap<String, CallbackId>`（默认返回空），由动态 builtin 实现，回读 `BindEvent`/`ClearEvent` 维护的绑定表。
   - `fn child_layout(&self, index: usize) -> Option<LayoutSpec>` 与 `fn child_meta(&self, index: usize) -> BTreeMap<String, ComponentValue>`（默认空），供容器回读子附加数据；或让 `children()` 返回的 `ComponentNode` 直接带上 layout/meta（更内聚，优先）。
2. 审计 `builtins.rs` 中每个内置组件，确保 `property_names()` 列全、`get_property()` 对每个可 set 的 prop 都可读（与 `set_property` 对称）。这是 `to_spec` 正确性的基础，也顺带修 inspect 快照裁剪那类信息丢失。

**验收**：对任意 builtin 树，`build(spec).to_spec() == spec`（round-trip 相等，用 `ComponentSpec: PartialEq` 断言）。这是整个改造的核心测试。

### 3.2 新增 `to_spec()`

在 `Component` trait 上加：

```rust
/// 从活实例派生声明式快照。默认实现按反射面组装；动态容器可覆写以直连内部结构。
fn to_spec(&self) -> Option<ComponentSpec> { None }
```

- 动态 builtin 提供实现：`type_name` + `tag()` → id + 遍历 `property_names()`/`get_property()` → props + `event_bindings()` → events + 递归 `children()` 各自 `to_spec()` + 附 layout/meta → children。
- `ComponentTree` 覆写 `dynamic_root_spec()`：不再返回 `&self.root`，改为返回由 `to_spec()` 现场生成的 spec（注意：签名要从 `Option<&ComponentSpec>` 改为返回 owned `Option<ComponentSpec>`，因为派生值无处借用——见 3.5 兼容性）。

### 3.3 移除 `root` 字段，重写 tree-ops 应用

`apply_ops_incremental` 现在依赖 `root` 做两件事，都要替换：

1. **回滚基线**：现用 `original_root.clone()`。改为**事务前 clone 一份 view 兜底**——进入前 `let backup = self.view.clone_boxed()`（需要 `Component: clone_boxed`，见下），失败时 `self.view = backup`。或让每个可失败的结构 op 保证"失败时 view 不变"（部分 op 如 `move_node_indexed` 注释已声称保持 view 完整），逐 op 校验后仅在真正破坏时回滚。
2. **形状校验 `view_shape_matches_spec(root_after_op, view)`**：不再需要拿 spec 比 view——因为不再有独立 root 需要保持一致。校验目标变成"op 是否成功应用到 view"，可用 op 本身的后置条件（如 Insert 后目标父节点 children 数 +1）替代。

`SetTree` 全量替换：直接 `self.view = registry.build(&spec)`，无 root 可留。

**关于回滚介质（已核实，结论确定）**：`Box<dyn Component>` **当前不可克隆**——`Component` trait 没有 `Clone` 约束、没有 `clone_boxed`、未接入 `dyn_clone`；`ComponentNode`（持有 `Box<dyn Component>`）自身不可 `Clone`（`node.rs` 中 `#[derive(Clone, Copy, …)]` 属于紧邻的 `ComponentId`，非 `ComponentNode`）；`tree.rs` 里所有 `.clone()` 克隆的都是 `ComponentSpec` 或 `CallbackRegistry`，无一克隆 view。

因此有两条路，且推荐已成唯一务实选择：

- **（推荐）临时 spec 快照回滚**：进入事务前 `let backup = self.view.to_spec()`，失败时 `self.view = registry.build(&backup)?`。这等价于旧的 `original_root.clone()` 行为，但真值仍是 view，spec 只作临时回滚介质、不常驻。**无需给全体 Component 加 Clone 约束**，改动最小。此路直接依赖 3.1/3.2 的无损 `to_spec()`——回滚正确性因此与 round-trip 测试绑定，是额外收益。
- **（不推荐）引入 dyn-clone**：给 `Component` 加 `clone_boxed`（或接 `dyn_clone` crate），事务前 clone view。代价是全体 Component 实现都要能克隆（含内部 `Binding`、缓存等），基础设施改动大、约束污染广，不划算。

结论：采用临时 spec 快照方案。

### 3.4 读接口统一

- `get_property` / `property_names`：已读 view，不动。
- `dynamic_root_spec()` / `root_spec()` 所有调用点（`file_dialog.rs:674`、`component_tag.rs:196`、`visibility.rs:263`、`border.rs:334`、`tab_window.rs:195/495`、`component.rs:399/609/703`）：语义从"读常驻 root"变为"派生当前 spec"。因返回值从借用变 owned，逐点改为持有临时变量。

### 3.5 绑定侧（Python/Node）

- spec 现在总是反映 view 当前状态，跨语言查询不再读到陈旧值——这是本改造对绑定的主要收益。
- 顺带修 M1（回调存活性）：与本方案独立，但可一并处理——把存活性过滤下沉到 `CallbackRegistry`（提供 `unregister`），让两个绑定共享语义。

---

## 4. 影响与风险

**收益**
- B4 两个症状（rebuild 吞输入、读接口矛盾）从根上消失。
- introspection / 跨语言查询永远看到当前真实状态。
- 顺带迫使反射面无损，改善 inspect 快照信息丢失（`DESIGN_REVIEW.md` 中 inspect M1）。

**风险 / 代价**
- **反射面无损是硬要求**：任何 builtin 漏报 prop / event，`to_spec` 就会丢信息，round-trip 测试是防线。工作量集中在审计 builtins。
- **`dynamic_root_spec` 返回类型改 owned**：波及约 8 处调用点，机械改动，编译器可导航。
- **`to_spec` 有序列化成本**：仅在出向边界（introspection、跨语言查询）调用，不在渲染热路径，可接受；如成瓶颈可加脏标记缓存。
- **回滚正确性**：新回滚（view 快照 / spec 临时快照）需覆盖所有 op 失败分支的测试，确保失败后 view 与"未应用该批 ops"一致。

**回滚介质选型结论（已核实）**：`Box<dyn Component>` 当前不可克隆（`Component` 无 Clone 约束、无 `clone_boxed`、未接 dyn-clone）。故采用 3.3 的"事务前 `to_spec()` 快照 + 失败时 build 回来"，不给 Component 加 Clone 约束，改动最小且真值仍单一（view）。引入 dyn-clone 的替代路径因约束污染面过大，不采纳。

---

## 5. 实施顺序（建议）

1. **反射面无损 + round-trip 测试**（3.1）——独立可测，不改行为，先落地。
2. **`to_spec()` 实现**（3.2）——依赖 1，加测试 `build(spec).to_spec() == spec`。
3. **`dynamic_root_spec` 改派生 + 调用点适配**（3.4 + 3.5 读侧）——依赖 2，此时 root 字段仍在但读接口已统一，B4 的"读矛盾"症状先消除。
4. **移除 `root` 字段 + 重写回滚/校验**（3.3）——最后做，消除"rebuild 吞输入"，完成单一真值。
5. （可选）绑定回调存活性下沉（M1）。

每步都可独立编译、独立测试、独立提交。第 3 步后 B4 已缓解一半，第 4 步彻底根治。
