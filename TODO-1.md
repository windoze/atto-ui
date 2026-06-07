# Node binding + React 风格 UI 库 任务列表

> 来源：`PLAN-1.md`（基于 `NODE_BINDING.md`，下称 §N 指其小节）。
> 说明：任务编号用 `NT*`（实现）/`NR*`（审阅）命名空间，与 `TODO-2.md`（editor-app，用 `T*`/`R*`）区分，避免撞号。每个实现任务 `NT` 后紧跟一个审阅任务 `NR`。
> 通用要求（每个 NT 完成前必须满足）：
> - Rust 侧：`cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test` 全绿。
> - TS/JS 侧（涉及 `packages/` 时）：`tsc --noEmit` 通过 + JS 测试全绿。
> 架构铁律：核心 crate `atto-ui` 永不依赖 tokio；不反向回调宿主（事件经 `CallbackRegistry` 轮询 `drainCallbacks`）；维持 `#![forbid(unsafe_code)]`（napi 宏 unsafe 局部豁免）；binding 与 `crates/atto-ui-python` 对称；所有跨语言 id 用 string handle。
> 行号以执行时快照为准，如有偏移以函数名/符号为准。
>
> **前置说明**：Node 复用的 `AppHost` Rust 能力（`send_event`/窗口管理/`set_property`/`snapshot`/`new_headless`）已由 agent-ui 计划（归档于 `docs/archive/2026-06-07-agent-ui`，其 T3–T5）落地；本列表不再重复实现核心 AppHost，只做 Node 侧暴露与按需小幅扩展。

---

## 阶段一：M0 脚手架 + M1 binding 核心

### [DONE] NT1 — `atto-ui-node` crate 脚手架 + napi build（B.0）
**文件**：新增 `crates/atto-ui-node/`（`Cargo.toml`、`build.rs`、`package.json`、`src/lib.rs`），根 `Cargo.toml`（workspace members）
**现状**：尚无 Node binding crate；workspace 已有 `crates/atto-ui-python`（pyo3）可作对称参照。
**步骤**：
1. `Cargo.toml`：`crate-type=["cdylib"]`，依赖 `napi`/`napi-derive`（`serde-json` feature）、`atto-ui`、`atto-ui-components`、`serde_json`；`build.rs` 调 `napi-build`。
2. 尽可能在 crate 头部照搬 Python crate 的 `#![forbid(unsafe_code)]` + 局部 `#![allow(unsafe_op_in_unsafe_fn)]` 策略。如果该策略与 napi-rs 冲突，可适当放宽。
3. `package.json` 配 `@napi-rs/cli`；加入根 workspace `members`。
4. 暴露 `#[napi] fn version() -> String` 作为冒烟点。
**测试**：`__test__/` 内 JS require 调用 `version()`；`napi build` 产出 `.node`。
**验收**：`cargo build -p atto-ui-node` 通过，workspace 构建不回归；JS 能加载并调用。
**完成记录（2026-06-07）**：
- 新增 `crates/atto-ui-node/`：`Cargo.toml` 配置 `cdylib`、`napi`/`napi-derive`、`atto-ui`、`atto-ui-components`、`serde_json` 与 `napi-build`；`build.rs` 调用 `napi_build::setup()`。
- 新增 `package.json` 的 `@napi-rs/cli` build/test 脚本、`main`/`types` 入口与 `napi.binaryName`；保留生成的 `index.js` / `index.d.ts` 供 JS 侧加载与类型冒烟，`.node` 产物由 `.gitignore` 排除。
- 新增 `src/lib.rs` 暴露 `#[napi] version() -> String`；因 napi-rs 宏展开会局部 `allow(unsafe_code)`，crate 级别采用 `#![deny(unsafe_code)]` + `#![allow(unsafe_op_in_unsafe_fn)]`，符合本任务允许的 napi-rs 冲突放宽。
- 新增 `__test__/version.cjs`，通过 package 入口 require 生成的 napi loader 并断言 `version()` 返回 `0.1.0`。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo build -p atto-ui-node`；`cargo test`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`；`node __test__/version.cjs`。

### [TODO] NR1 — 审阅 NT1
- 确认 crate-type/feature/依赖正确，workspace 不引入 tokio。
- 确认 `.node` 产物可被 Node require。
- 运行 `cargo build --workspace` + JS 冒烟。

### [TODO] NT2 — serde 数据转换层（B.2）
**文件**：`crates/atto-ui-node/src/convert.rs`
**现状**：核心类型 `ComponentSpec`/`ComponentSpecChild`/`LayoutSpec`/`ComponentValue`/`TreeOp`/`CallbackInvocation`/`ComponentSchema` 均已 `derive(Serialize,Deserialize)`（`src/runtime/spec.rs`）。Python 侧是手写 dict→struct（近千行），Node 改走 serde。
**步骤**：
1. `Object`/`JsUnknown` ↔ `serde_json::Value` ↔ 上述类型（`napi` 的 `serde-json` 支持）。
2. 约定 JS 侧 `TreeOp` 形态为 discriminated union（`{op:"set_prop",id,name,value}` 等，§6.2）；snake_case 与核心枚举对齐。
3. `ComponentValue` 全分支：bool/number/string/string[]/string[][]/bytes/list/map（§6.2 类型映射）。
**测试**：Rust 单测——`ComponentSpec`/`TreeOp`/`ComponentValue` round-trip；每种 op 解析正确。
**验收**：JS 对象与核心类型双向转换正确，且转换代码量显著小于 Python 手写路径。

### [TODO] NR2 — 审阅 NT2
- 确认所有 `TreeOp` 变体、`ComponentValue` 分支均覆盖且与核心定义一致。
- 确认错误（缺字段/类型不符）有清晰报错而非 panic。
- 运行转换单测。

### [TODO] NT3 — id handle 包装 + 错误映射（B.3 / B.4）
**文件**：`crates/atto-ui-node/src/ids.rs`、`src/error.rs`
**现状**：`CallbackId`/`WindowId` 是 u64 newtype；napi 把 u64 映射为 JS `BigInt`（§10.5）。节点 id 本身是 `String`（`ComponentSpec.id`），无需包装。
**步骤**：
1. `ids.rs`：`CallbackId`/`WindowId` ↔ 不透明 **string handle**，内部 Map 双向解析；JS 侧只做相等/查表。
2. `error.rs`：`TreeError`/`anyhow::Error` → `napi::Error`，信息透传到 JS throw。
**测试**：单测 handle 双向解析一致；错误转换保留消息。
**验收**：JS 侧不接触 BigInt、不做 id 算术；Rust 错误能在 JS 以 Error 抛出。

### [TODO] NR3 — 审阅 NT3
- 确认 handle 包装无泄漏（窗口/回调销毁后 handle 失效处理合理）。
- 确认错误信息不丢失、不暴露内部细节。
- 运行单测。

### [TODO] NT4 — `#[napi] AppHost` 全方法暴露（B.1）
**文件**：`crates/atto-ui-node/src/lib.rs`
**现状**：Rust `::atto_ui::app::AppHost` 已具备 `new`/`new_headless`/`step`/`drain_callbacks`/`add_dynamic_window`/`apply_tree_ops`/`get_property`/`set_property`/`send_event`/窗口管理/`snapshot`/`schemas`（agent-ui 归档计划 T3–T5 落地）。
**步骤**：
1. 接线全部方法（签名见 PLAN-1 §1.2 B.1），构造时调 `atto_ui_components::register_all_components()`。
2. 构造默认 `tickRate=0`（非阻塞）、隐藏光标、鼠标捕获；支持 `headless` 选项。
3. 新增 `alloc_callback() -> String`（为事件 prop 申请 `CallbackId` handle，供 React 库 §10.8）。
4. 用 NT2/NT3 的转换与 handle 包装贯通参数与返回值。
**测试**：headless 冒烟（JS）——建窗口→`applyTreeOps` 改 text→`step`→`snapshot()` 断言文本；`drainCallbacks` 取回注入事件。
**验收**：JS 能用命令式 spec/op 驱动一个 headless 窗口，能力与 Python `PyAppHost` 对称。

### [TODO] NR4 — 审阅 NT4
- 确认方法集与 Python 对称，签名与 §6.1 一致。
- 确认 `step()` 在 `tickRate=0` 下非阻塞；`drainCallbacks` 载荷（callbackId/targetId/event/payload）齐全。
- 确认 headless 路径不依赖真实 PTY。
- 运行 JS 冒烟 + 相关单测。

### [TODO] NT5 — `@atto-ui/core` native 加载（L.1）
**文件**：新增 `packages/core/`（`index.ts`、`native.d.ts`）
**现状**：尚无 npm 包；napi 可自动生成 `.d.ts`。
**步骤**：
1. 加载平台 `.node`，re-export napi 生成的类型声明。
2. 暴露 `AppHost` 类型与 `ComponentSpec`/`TreeOp`/`ComponentValue`/`CallbackInvocation` 的 TS 类型。
**测试**：`tsc --noEmit` 通过；JS 端 import 并跑通 NT4 的 headless 冒烟。
**验收**：`@atto-ui/core` 可作为 React 库（阶段三起）的底层依赖。

### [TODO] NR5 — 审阅 NT5
- 确认类型声明与 Rust 侧一致、无 any 泄漏。
- 确认跨平台 `.node` 加载路径正确（为 P 阶段分发铺垫）。
- 运行 `tsc` + 冒烟。

---

## 阶段二：M2 runtime 改动（可与阶段一并行，改 `atto-ui` 核心）

### [TODO] NT6 — `TreeOp::InsertBefore` 锚点版插入（R.1）
**文件**：`src/runtime/spec.rs`、`src/runtime/tree.rs`
**现状**：现有 `Insert{parent_id,index,child}` 用数字 index；React `insertBefore` 给的是锚点节点引用，且重排对已挂载节点再次 `insertBefore`（§10.4 两个 gap）。
**步骤**：
1. `spec.rs`：`TreeOp` 新增 `InsertBefore { parent_id: String, anchor_id: Option<String>, child: ComponentSpecChild }`。
2. `apply_tree_op`：`anchor=None`→append；给定 anchor→解析为 index 插入；若 `child` id 已存在树中→等价 `Move`（先 detach）。
3. `tree.rs` `apply_ops_incremental`：为 `InsertBefore` 加增量分支（参照现有 `Insert`/`Move`），避免全量重建。
4. 向后兼容：旧 `Insert{index}` 与 Python 路径不变。
**测试**：单测覆盖 append / insert-before-anchor / 已存在节点→move 三态；增量路径不触发全量重建。
**验收**：三态行为正确；现有 runtime 测试与 Python 路径全绿。

### [TODO] NR6 — 审阅 NT6
- 确认 anchor 解析、move 等价语义与"不能移进自身子树"保护正确。
- 确认增量分支不误判结构变更、不全量重建。
- 确认对现有 `Insert`/Python 零影响。
- 运行 runtime 单测 + 全 workspace。

### [TODO] NT7 — `RichText` + `TextSpan` 结构化富文本（R.2）
**文件**：`src/text/styled_text.rs`、`src/widgets/`（新增）、`src/runtime/builtins.rs`
**现状**：`Text`/`StyledLabel` 均 `allow_children(false)`、文本走 prop；`styled_text.rs` 的 `StyledTextSegment` 渲染管线（`spans_from_segments`/`slice_segments`/`hit_test_link`）目前仅由 `parse_inline(markdown串)` 喂入（§10.7）。
**步骤**：
1. `styled_text.rs`：为 `StyledTextSegment` 增加可由结构化字段直接构造的入口（当前 `pub(crate)`）。
2. `TextSpan` 组件：props 为结构化 flags（`text`/`bold`/`italic`/`underline`/`strike`/`color?`/`href?`），`allow_children(false)`；注册到 builtins。
3. `RichText` 容器：`allow_children(true)`，build 时遍历 `TextSpan` 子节点→`Vec<StyledTextSegment>`→`spans_from_segments` 渲染；相邻同 style 合并、空 span 清理。
4. `href` 命中复用 `hit_test_link`，发 `link` 事件（payload=url）。
**测试**：headless 快照——粗/斜/下划线/删除线/链接渲染正确；PTY——点击链接触发回调。
**验收**：富文本以结构化子节点驱动，复用既有 segment 渲染管线，无需 markdown 转义。

### [TODO] NR7 — 审阅 NT7
- 确认渲染管线复用（未重写宽字符/截断逻辑）。
- 确认相邻片段合并、空 span 清理、链接命中正确。
- 确认 `Text`/`StyledLabel`/`MarkdownViewer` 等现有文本组件不回归。
- 运行快照 + PTY。

---

## 阶段三：M3 reconciler MVP + M4 主循环

### [TODO] NT8 — react-reconciler HostConfig 骨架 + 节点 id + 静态渲染（U.1）
**文件**：新增 `packages/react/`（`src/reconciler.ts`、`src/host.ts`）
**现状**：尚无 React 库；除 `SetTree` 外所有 `TreeOp` 靠 `ComponentSpec.id` 定位，React host 节点无天然 id（§10.4）。
**步骤**：
1. 引入 `react`/`react-reconciler`，搭 mutation 模式 HostConfig 骨架（LegacyRoot）。
2. host instance：`{ id, type, props, children, windowId }`；`createInstance` 时自增计数器→string，写入将提交的 `ComponentSpec.id`。
3. 子树首次挂载→构造 `ComponentSpec` 树→提交（`SetTree` 或批量插入）。
4. `finalizeInitialChildren`→false；`getPublicInstance`→instance；调度类方法代理 `setTimeout`/`clearTimeout`。
**测试**：reconciler 单测——渲染 `<vstack><label/></vstack>` 断言产出的 spec/ops；headless 渲染出文本。
**验收**：静态 React 树能渲染到一个窗口。

### [TODO] NR8 — 审阅 NT8
- 确认节点 id 稳定、唯一、随实例生命周期管理。
- 确认 HostConfig 必需方法齐备、mutation 模式配置正确。
- 运行 reconciler 单测 + headless。

### [TODO] NT9 — props/子节点增删/事件 op 映射（U.1）
**文件**：`packages/react/src/host.ts`、`src/reconciler.ts`
**现状**：NT8 已能静态渲染；需支持更新与增量。依赖 NT6 的 `InsertBefore`（或回退 index 版）。
**步骤**：
1. `prepareUpdate`/`commitUpdate`：props diff→批 `SetProp`；事件 prop 增删→`BindEvent`/`ClearEvent`。
2. 子节点：`appendChild`/`insertBefore`→`InsertBefore`（已挂载节点→Rust 侧等价 Move）；`removeChild`→`Remove`；`clearContainer`→批量 `Remove`/`SetTree(空)`。
3. children 顺序镜像（若用 index 版需据此换算）。
4. op 累积：commit 期间 push 进 buffer，`resetAfterCommit` flush（分桶在 NT13 完善）。
**测试**：reconciler 单测——`useState` 改 text→`SetProp`；列表增删/重排→正确 op 序列；事件 bind/clear 时机正确。
**验收**：动态更新、增删、重排、事件绑定均产出正确 `TreeOp`。

### [TODO] NR9 — 审阅 NT9
- 确认重排的 move 判定正确（已挂载→Move，新建→Insert）。
- 确认事件 prop 仅在增删时 Bind/Clear（不每次 render 重绑）。
- 确认 diff payload 精确（无多余 SetProp）。
- 运行 reconciler 单测。

### [TODO] NT10 — `render()` + tick 主循环（U.2）
**文件**：`packages/react/src/render.ts`
**现状**：`AppHost::step()` 在 `tickRate=0` 下非阻塞（§5.2）。
**步骤**：
1. `render(element, { cols?, rows?, singleWindow? })`：建 `AppHost`→建 container→`createContainer(LegacyRoot)`→`updateContainer`。
2. tick 微循环（`setImmediate`）：`step()`→（NT11 分发）→React flush→op flush；返回 `RenderHandle{stop,host}`。
3. 退出（Ctrl+Q→`step` 返回 false）→cleanup 恢复终端。
**测试**：PTY——启动渲染、退出干净恢复终端。
**验收**：React 树能持续渲染并响应 tick；进程退出不留终端残状态。

### [TODO] NR10 — 审阅 NT10
- 确认微循环让出事件循环（不阻塞 Promise/IO）。
- 确认退出路径恢复终端（raw mode/光标/鼠标）。
- 运行 PTY。

### [TODO] NT11 — 事件分发桥（U.3）
**文件**：`packages/react/src/events.ts`
**现状**：UI 事件经 `CallbackRegistry` 收集，`drainCallbacks()` 拉取（§10.8）。
**步骤**：
1. `callbackId → 最新 handler` Map：`callbackId` 绑定一次、Map 始终指向最新闭包；仅事件 prop 增删时 `BindEvent`/`ClearEvent`。
2. tick 内 `drainCallbacks()`→查表→调 handler→`setState`→React flush→op flush。
3. 组件卸载时 `ClearEvent` + 回收 callbackId。
**测试**：reconciler/集成单测——handler 不重复 bind；卸载后不再触发；PTY——点击 Button→`onClick`→`setState`→屏幕更新（计数器 +1）。
**验收**：UI 事件→React 状态→重渲染闭环成立；无 handler 泄漏/stale。

### [TODO] NR11 — 审阅 NT11
- 确认闭环无重入/丢事件；callbackId 回收正确。
- 确认 LLM 流式与 UI 共存（`for await` 灌 `setState` 不阻塞，§5.2）。
- 运行 PTY + 流式示例。

---

## 阶段四：M5 文本子系统

### [TODO] NT12 — React 文本组件（U.5）
**文件**：`packages/react/src/text.ts`
**现状**：NT7 已提供 `RichText`/`TextSpan`；`MarkdownViewer` 已注册（`crates/atto-ui-markdown`）。
**步骤**：
1. `createTextInstance(text)`→`TextSpan`；`commitTextUpdate`→`TextSpan` 的 `SetProp text`。
2. 内联组件 `<Text>`/`<B>`/`<I>`/`<U>`/`<S>`/`<Link href>`→设置子 `TextSpan` style flags；`<Text>` 作 `RichText` 容器。
3. `<Link href onClick>`→绑 `link` 事件，payload=url 路由到 `onClick`。
4. `<Markdown>{md}</Markdown>`→`MarkdownViewer`（props `markdown`）。
**测试**：快照——`<B>` 粗体、`<Text>hi {name}</Text>` 合并、块级 markdown；PTY——点击链接触发。
**验收**：React 文本/内联样式/markdown 渲染正确，文本片段在 Rust 侧合并。

### [TODO] NR12 — 审阅 NT12
- 确认文本节点合并发生在 Rust 侧（`RichText`），JS 仅产 `TextSpan`。
- 确认链接事件 payload 与 onClick 路由正确。
- 确认 `<Markdown>` 与 viewer 属性映射一致。
- 运行快照 + PTY。

---

## 阶段五：M6 Window 映射

### [TODO] NT13 — 虚拟 DesktopContainer + `<Window>` host 节点 + op 分桶（U.4）
**文件**：`packages/react/src/desktop.ts`、`src/host.ts`（路由）
**现状**：`Desktop` 刻意非 spec 树；window 高频增删；`apply_tree_ops` 是 per-window（§10.6）。React 单一 root 约束在 fiber 层（§10.2）。
**步骤**：
1. 虚拟 `DesktopContainer`：只接受 `<Window>`/`<MenuBar>`/`<StatusBar>` 作直接子节点。
2. 容器版 HostConfig 方法：`appendChildToContainer(desktop,<Window>)`→`addDynamicWindow` 并存 `windowId`；`removeChildFromContainer`→`closeWindow`；`<MenuBar>`/`<StatusBar>`→命令式 set 固定槽位；普通组件挂 root→运行期报错。
3. `<Window title rect>` props 改→`move`/`resize`/`setTitle`。
4. op 路由：instance 从最近 `<Window>` 祖先继承 `windowId`；`resetAfterCommit` 按 `windowId` 分桶，逐窗口 `applyTreeOps`。
5. `singleWindow:true`：自动包全屏 `<Window>`。
6.（可选）Portal：`createPortal(children, windowContainer)`。
**测试**：PTY——开/关窗口、改 title/rect；reconciler 单测——两窗口 op 各归各位；单测——Context 跨窗口贯通。
**验收**：多窗口声明式管理；window 增删不进 TreeOp；跨窗口共享状态成立。

### [TODO] NR13 — 审阅 NT13
- 确认 DesktopContainer 仅接受合法子节点，非法子节点编译期/运行期被拦。
- 确认 op 分桶无串窗口；windowId 继承正确。
- 确认 `singleWindow` 与多窗口路径一致。
- 运行 PTY + 单测。

---

## 阶段六：M7 组件库 + TS 类型

### [TODO] NT14 — host 组件库 + JSX 类型 + 受控输入（U.6）
**文件**：`packages/react/src/components.ts`、`src/jsx.d.ts`
**现状**：内置组件已注册（Button/TextBox/ListBox/TableView/VStack/HStack/Grid 等）；需 React wrapper 与类型。
**步骤**：
1. intrinsic elements + `jsx.d.ts`；wrapper：`<Button onClick>`/`<TextBox value onChange>`/`<ListBox>`/`<Table>`/`<VStack>`/`<HStack>`/`<Grid>`。
2. 统一事件 prop 约定（`onClick`/`onChange`/`onSelect`）→ atto-ui 事件名映射。
3. 在 napi 生成的 native `.d.ts` 上扩展组件 props 类型。
4. 受控输入回环：`<TextBox value onChange>`，确认外部 `SetProp value` + 变更事件不打架（§10.7 风险）。
**测试**：`tsc` 通过；PTY——各组件交互；受控 TextBox 输入正确。
**验收**：常用组件有类型化 React 封装；受控输入无回环抖动。

### [TODO] NR14 — 审阅 NT14
- 确认事件 prop 约定一致、类型精确。
- 确认受控输入不出现光标跳动/重复字符。
- 运行 `tsc` + PTY。

### [TODO] NT15 — `@atto-ui/core` 命令式构造器（L.2）
**文件**：`packages/core/src/`
**现状**：低层用法需不依赖 react 的 spec 构造器（§6.4）。
**步骤**：提供 `VStack(...)`/`Text(...)`/`Button(...)` 等薄包装 spec 对象的构造器（类型安全），供低层用法与作为 React 库底层。
**测试**：单测构造 spec 与手写 JSON 等价；`tsc` 通过。
**验收**：不使用 React 也能类型安全地构树。

### [TODO] NR15 — 审阅 NT15
- 确认构造器产出的 spec 与核心类型一致、无字段遗漏。
- 运行单测 + `tsc`。

---

## 阶段七：M8 测试 + 示例

### [TODO] NT16 — reconciler 单测矩阵（T.1）
**文件**：`packages/react/__test__/`
**步骤**：mount/update/增删/重排/事件 bind-clear → 断言产出的 `TreeOp` 序列（纯 JS，不进 native）。
**测试**：覆盖 §10.4 映射表每一类操作；含 move 判定、事件 clear 时机边界。
**验收**：HostConfig 行为有回归护栏，CI 内运行。

### [TODO] NR16 — 审阅 NT16
- 确认矩阵覆盖全（无遗漏 op 类型/边界）。
- 确认断言精确（op 顺序、分桶）。
- 运行单测。

### [TODO] NT17 — PTY 端到端（T.2）
**文件**：`crates/atto-ui-node/__test__/` 或 JS e2e + 复用 `crates/atto-ui-test-host`
**步骤**：计数器、表单（受控输入）、列表增删、多窗口——经真实/headless 路径驱动并断言屏幕。
**测试**：PTY/headless e2e。
**验收**：关键交互路径端到端可验证。

### [TODO] NR17 — 审阅 NT17
- 确认 e2e 走真实 Rust 分发路径（非 JS 侧模拟）。
- 确认多窗口/受控输入覆盖。
- 运行 e2e。

### [TODO] NT18 — 示例 app（含流式聊天）（T.3）
**文件**：`examples/`（JS/TS）
**步骤**：计数器、待办表单、**流式聊天**（Anthropic/OpenAI SDK 灌 token，验证 §5.2 UI 与流式共存）。
**测试**：手动运行 + 截图；流式高频更新不卡（必要时引入限频重绘/`stepDrainInput`）。
**验收**：示例可运行，覆盖 state/事件/受控/流式。

### [TODO] NR18 — 审阅 NT18
- 确认示例可独立运行、依赖声明完整。
- 确认流式与 UI 不互相阻塞。
- 性能 sanity（大列表/高频流式）。

---

## 阶段八：M9 打包分发

### [TODO] NT19 — 跨平台预编译 + npm 包（P.1 / P.2）
**文件**：`crates/atto-ui-node/package.json`、`packages/*/package.json`、npm 平台子包
**步骤**：
1. `@napi-rs/cli` 平台矩阵：darwin-arm64/x64、linux-x64-gnu、win32-x64-msvc。
2. 主包 + `optionalDependencies` 指向各平台二进制包（§7.2）。
**测试**：本地各平台 `napi build`；`npm pack` 结构正确。
**验收**：`npm install` 按平台拉取二进制，无需本地 Rust 工具链。

### [TODO] NR19 — 审阅 NT19
- 确认平台矩阵与 optionalDependencies 配置正确。
- 确认无 native 工具链时安装可用。
- 验证打包产物。

### [TODO] NT20 — CI 流水线 + Bun/Deno + 文档（P.3）
**文件**：CI 配置、`README`、API 文档
**步骤**：
1. CI：交叉编译 + tag→发布；运行 reconciler 单测与 e2e。
2. Bun/Deno 兼容性实测（N-API 理论兼容，验 raw-mode 终端行为，§11）。
3. README + 快速开始 + API 文档。
**测试**：CI 全绿；Bun/Deno 冒烟。
**验收**：可一键发布；多运行时验证通过；文档可上手。

### [TODO] NR20 — 审阅 NT20
- 确认 CI 覆盖编译/测试/发布全链路。
- 确认 Bun/Deno 行为与 Node 一致或已记录差异。
- 确认文档与实际 API 同步。

---

## 测试与回归约定

- 每个 NT 完成前：Rust 侧 `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test` 全绿；TS 侧 `tsc --noEmit` + JS 测试全绿。
- UI 行为类测试优先走 `atto-ui-test-host` PTY / `AppHost::new_headless` 快照，保证确定性。
- reconciler 纯逻辑（op 映射、move 判定、事件 bind/clear）用 JS 单测，不进 native。
- runtime 改动（NT6/NT7）需保证现有 `atto-ui` 测试与 `crates/atto-ui-python` 路径不回归。

## 执行顺序

1. M0+M1：NT1→NR1→NT2→NR2→NT3→NR3→NT4→NR4→NT5→NR5
2. M2（可与上一组并行，不同 crate）：NT6→NR6→NT7→NR7
3. M3+M4：NT8→NR8→NT9→NR9→NT10→NR10→NT11→NR11
4. M5：NT12→NR12
5. M6：NT13→NR13
6. M7：NT14→NR14→NT15→NR15
7. M8：NT16→NR16→NT17→NR17→NT18→NR18
8. M9：NT19→NR19→NT20→NR20

> **MVP 切片**：完成 NT1–NT4 + NT8–NT11（binding 核心 + reconciler + 主循环 + 事件桥）即可演示单窗口 React 计数器（文本先用过渡方案），作为第一个可演示节点。
