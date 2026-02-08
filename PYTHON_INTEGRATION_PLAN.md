# Python 集成方案（pyo3 + maturin）

> 目标：在不破坏现有 `atto-ui` 架构的前提下，引入“可完全动态（fully dynamic）”的 Python 集成能力，支持：
> - 通过 Python 进行组件树的动态创建/修改/替换
> - 动态读取/设置属性（含类型与可写性元信息）
> - 动态注册 Python 回调，并在 UI 事件中调用
> - 保持 Rust 核心与语言绑定解耦，便于未来支持其他语言

---

## 1. 现状审视：仍不 fully dynamic 的点（基于当前代码）

1. **属性元信息还不完整**
   - 已有 `ComponentProperties` + `#[component_properties]` 能提供**属性名**和 `ComponentValue` 读写，但**类型/可写性/事件/动作**元信息仍缺失。
   - `ComponentValueCodec` 解决了“值转换”，但没有统一的 schema 供动态系统查询。

2. **动态组件树能力已有雏形，但未接入 Desktop/WM**
   - `atto-ui-runtime` 已提供 `ComponentSpec/TreeOp/ComponentRegistry/CallbackRegistry`，`src/runtime` 也有 `ComponentTree` + 增量更新逻辑。
   - 但这些能力尚未与 `Desktop/Window` 对接为“动态窗口根组件”，Python 无法直接驱动 UI。

3. **回调跨语言桥接未打通**
   - 运行时已有 `CallbackRegistry` 和 `CallbackInvocation`，但多数控件仍以 Rust 闭包为主。
   - 需要统一接入 `CallbackHandle`，并把事件队列暴露给 Python。

4. **稳定 id 仍是软约束**
   - `ComponentSpec.id` 仍可选，动态系统无法保证稳定寻址。

5. **语言无关层已存在，但缺少“面向绑定”的上层 API**
   - `atto-ui-runtime` 已具备语言无关数据结构，但缺少清晰的 host API（如 `RuntimeHost` / `DynamicWindow`）承接 Python 绑定的调用。

---

## 2. 设计原则

1. **核心库不依赖 Python**：Python 绑定应置于独立 crate。
2. **动态能力以“声明式树 + 补丁”驱动**：Python 通过 `ComponentSpec` / `TreeOp` 更新 UI。
3. **组件元信息可查询**：属性类型、读写能力、事件列表可 introspection。
4. **回调通过 ID + 队列派发**：避免 Rust 组件直接持有 Python 对象。
5. **可扩展**：接口对其他语言绑定友好。

---

## 3. 目标架构（分层）

```
+--------------------------+
|      atto-ui-python       |  (pyo3, maturin)
|  - Py API / wrappers      |
+-------------+------------+
              |
+-------------v------------+
|     atto-ui-runtime       |  (语言无关桥接层)
|  - ComponentValue         |
|  - ComponentSchema        |
|  - ComponentRegistry      |
|  - ComponentSpec / TreeOp |
|  - CallbackRegistry       |
+-------------+------------+
              |
+-------------v------------+
|       atto-ui 核心         |
|  - Component / Desktop    |
|  - ComponentProperties    |
|  - Widgets / Layout       |
+--------------------------+
```

> `atto-ui-runtime` 作为语言无关桥接层，Python/其它语言均基于该层实现绑定。

---

## 4. 核心改造方向与建议

### 4.1 强化运行时桥接层（已有 `atto-ui-runtime`）

目标：把现有 runtime 结构升级为“动态 UI 的事实 API”。

- `ComponentValue` 作为统一动态值类型（已存在）。
- `ComponentSchema` 补齐：属性类型、读写能力、事件列表、动作列表。
- `ComponentRegistry` 提供类型名 -> 构造器 + schema。
- `CallbackRegistry` 作为跨语言回调队列（仅保存 id，不保存 Python 对象）。

### 4.2 组件元信息与属性 introspection

- 用 `ComponentProperties` / `#[component_properties]` 统一导出属性名与读写能力。
- 新增/扩展宏或注册表：将组件属性映射为 `ComponentSchema`。
- 为事件与动作补充统一描述：`EventMeta` / `ActionMeta`。
- `ComponentValueCodec` 继续作为值转换入口。

### 4.3 动态组件树管理

建议接入点：
- 在 `Desktop` 中加入 `DynamicWindow` / `RuntimeWindow`：
  - 内部持有 `ComponentTree`（基于 `ComponentSpec`）。
  - 暴露 `apply_tree_ops(...)`、`rebuild(...)`。
- 统一要求动态树节点带稳定 id，或由 host 自动生成并回写给 Python。

### 4.4 回调体系（Python 动态回调）

目标：Python 注册函数，UI 事件触发时进入队列，再由 Python 拉取并执行。

方案：
- 组件内部只保存 `CallbackId`，触发事件时写入 `CallbackRegistry`。
- Python 侧拉取 `CallbackInvocation` 并调用对应 Python 函数（持 GIL）。
- 为控件补充 `*_callback(CallbackHandle)` 入口（Button/TextBox 等）。

### 4.5 事件循环与驱动模式

提供两种模式：
1. **Rust 主循环驱动**：Python 仅下发 TreeOp + 回调注册。
2. **Python 驱动**：暴露 `App.step()` / `App.run()`，Python 控制主循环。

需要桥接：
- 新增 `RuntimeHost::pump()` 或 `AppHost::step()` 作为通用入口。

---

## 5. Python 绑定设计（pyo3 + maturin）

### 5.1 Python API 草案

```python
app = atto_ui.App()

root = app.root()
root.set_tree({
    "type": "VStack",
    "id": "main",
    "props": {"spacing": 1},
    "children": [
        {"type": "Label", "id": "title", "props": {"text": "Hello"}},
        {"type": "Button", "id": "ok", "props": {"label": "OK"}}
    ]
})

@app.on("ok", "click")
def handle_ok(event):
    print("clicked", event)

app.run()
```

### 5.2 Maturin 工程结构

- 新 crate：`crates/atto-ui-python`
- `pyproject.toml` + `Cargo.toml` 配置 maturin
- 与核心 crate 通过 `atto-ui-runtime` 交互

---

## 6. 里程碑计划（建议分阶段）

### 阶段 1：运行时接口与元信息
- 补齐 `ComponentSchema`（属性/事件/动作）。
- 将 `ComponentProperties` 映射为 schema（宏或注册表）。
- 完善 `ComponentValueCodec` 覆盖范围与错误信息。

### 阶段 2：动态组件树
- 基于 `ComponentSpec` + `TreeOp` 打通 `Desktop` 的动态窗口根。
- 为动态树引入稳定 id 策略。

### 阶段 3：回调系统与派发
- 统一控件回调入口使用 `CallbackHandle`。
- 暴露回调队列给 Python。

### 阶段 4：Python 绑定
- `atto-ui-python` + pyo3
- 暴露 `App`, `Window`, `Node`, `Value`, `Event` 等 API
- 集成 maturin 打包

### 阶段 5：测试与示例
- Rust 单元测试：`TreeOp`、`ComponentSchema`、`CallbackRegistry`。
- Python 端 pytest：
  - 动态树构建
  - 属性读写
  - 回调触发

---

## 7. 风险与注意事项

1. **线程与 GIL**：Python 回调必须在持 GIL 的线程执行。
2. **性能与频繁重建**：应以补丁式更新避免全量 rebuild。
3. **稳定 id**：动态系统必须可预测地寻址节点。
4. **动态 schema 一致性**：schema 变更需要配套版本策略或校验逻辑。

---

## 8. 建议的代码落点（便于后续实现）

- `crates/atto-ui-runtime/`：语言无关桥接层
- `src/runtime/mod.rs`：动态树构建/更新（`ComponentTree`）
- `src/component_api.rs`：`ComponentValueCodec` + 动态错误/命令模型
- `crates/atto-ui-python/`：pyo3 + maturin 绑定
- `src/widgets/*`：统一回调入口（`CallbackHandle`）

---

## 9. 需要进一步确认的问题

- Python 侧运行模式：主循环驱动还是 step/poll？
  - 主循环采用 step/poll 机制会更灵活一些。
- 组件注册策略：是否所有内置组件都提供 schema + factory？
  - 是，所有内置组件都应提供 schema 和 factory，以便动态创建和管理。
- 回调参数是否需要携带 buffer snapshot / bounds 等细节？
  - 不需要，回调参数应尽量简洁，其他信息通过查询接口获得。

---

> 以上为可执行的设计与阶段规划。如确认方向，下一步可拆分为具体任务列表并开始实现。
