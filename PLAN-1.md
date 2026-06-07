# Node binding + React 风格 UI 库 实现计划

> 本计划把 `NODE_BINDING.md` 的**设计**落到**具体 crate / npm 包**，确立清晰的依赖边界与实施顺序。
> `NODE_BINDING.md` 解释「为什么这样设计」(下称 §N)，本文解释「做哪些文件、写哪些接口、按什么顺序、怎么验收」。
> 四条架构约束：
> 1. 通用/运行时能力放核心 crate `atto-ui`；binding 不新增核心能力(§2)。
> 2. 核心 crate 保持 **std-only，永不依赖 tokio**；async 由 Node 事件循环承担(§5)。
> 3. binding 与 Python(`crates/atto-ui-python`)**对称**：AppHost 能力一处实现、两处暴露。
> 4. React 库完全建立在 binding 的 `AppHost` API 之上，binding 层可独立交付(命令式 spec/op 即可用)。

---

## 0. 依赖关系总览

```
                 atto-ui (core, std-only, 无 tokio)
                 └─ runtime: ComponentSpec / TreeOp / ComponentValue / AppHost
                    ＋本计划新增: TreeOp::InsertBefore、RichText/TextSpan
                        ▲                         ▲
            ┌───────────┘                         └────────────┐
   crates/atto-ui-python                           crates/atto-ui-node
   (pyo3, 已存在)                                   (napi-rs binding) ← 新建
                                                              ▼
                                                  @atto-ui/core  (TS 低层包)
                                                  加载 .node + 命令式 spec API
                                                              ▼
                                                  @atto-ui/react (React 风格库)
                                                  react-reconciler HostConfig
                                                              ▼
                                                       Node/TS agent
```

**铁律**
- `atto-ui` 永不依赖 tokio；Node 的 async 完全由其事件循环承担，binding 不引入 tokio。
- 不反向回调宿主：UI 事件经 `CallbackRegistry` 收集，JS 每 tick `drainCallbacks()` 拉取(§5.3、§10.8)。
- 维持 `#![forbid(unsafe_code)]`，napi 宏的 unsafe 用局部 `#![allow(unsafe_op_in_unsafe_fn)]` 豁免(同 Python crate)。
- runtime 改动**向后兼容**：旧 `Insert{index}` 与 Python 路径不受影响。
- React 的 Context/state 共享在 fiber 层，与 host 节点物理切分无关——单一 `createContainer` 即可跨窗口贯通(§10.2)。

---

## 1. 能力点 → crate / 包 映射

### 1.1 核心 crate `atto-ui`(runtime 改动，std-only)

> 两处改动支撑 React 库；均需配套单测，且不破坏现有 Python 路径。可与 1.2 并行。

**R.1 `TreeOp::InsertBefore`(锚点版插入，§10.4)**
- `src/runtime/spec.rs`：`TreeOp` 新增 `InsertBefore { parent_id: String, anchor_id: Option<String>, child: ComponentSpecChild }`。
- `apply_tree_op`：`anchor=None`→append；给定 anchor→解析为 index 插入；若 `child` 的 id 已存在树中→等价 `Move`(先 detach 再插)。
- `apply_ops_incremental`(`src/runtime/tree.rs`)：为 `InsertBefore` 增加增量分支(参照现有 `Insert`/`Move`)，避免全量重建。
- **动机**：把 index 计算与 move 判定从 JS 移到 Rust，规避 React `insertBefore` 锚点语义与批内 index 漂移(§10.4 两个 gap)。
- 验收：单测覆盖 append / insert-before / 已存在节点→move 三态；现有 runtime 测试全绿。

**R.2 `RichText` + `TextSpan`(结构化富文本，§10.7)**
- 复用 `src/text/styled_text.rs` 的 `StyledTextSegment` 渲染管线(`spans_from_segments`/`slice_segments`/`hit_test_link`)，仅新增「从结构化子节点构造 segments」的输入路径。
- `TextSpan`(`src/widgets/` + `src/runtime/builtins.rs` 注册)：props 为结构化 flags(`text`/`bold`/`italic`/`underline`/`strike`/`color?`/`href?`)，`allow_children(false)`。
- `RichText`：`allow_children(true)`，build 时遍历 `TextSpan` 子节点→`Vec<StyledTextSegment>`→渲染；相邻同 style 合并、空 span 清理(「合并在 binding 侧」)。
- `href` 命中复用 `hit_test_link`，发 `link` 事件(payload=url)。
- **动机**：用结构化 flags 而非 markdown 标记串，避免用户文本里的 `*`/`[` 被误解析(§10.7)。
- 验收：headless 快照渲染粗/斜/链接正确；PTY 点击链接触发回调。

### 1.2 新 crate `crates/atto-ui-node`(napi-rs binding)

> 与 Python `PyAppHost` 对称(§6.1)。走 serde 大幅减少手写转换(§4.2)。可独立交付：命令式 spec/op 即可驱动 UI。

**B.0 脚手架**
- `Cargo.toml`(`crate-type=["cdylib"]`)、`build.rs`(napi-build)、`package.json`(`@napi-rs/cli`)；加入 workspace `members`。
- crate 头部照搬 Python crate 的 `forbid(unsafe_code)` + 局部豁免策略。
- 验收：`napi build` 产出 `.node`，JS `require` 调 `version()` 成功。

**B.1 `#[napi] AppHost` 方法(接线 `::atto_ui::app::AppHost`)**
```rust
#[napi] impl AppHost {
  #[napi(constructor)] pub fn new(config: Option<AppHostConfig>) -> Result<Self>; // tickRate 默认 0=非阻塞
  #[napi] pub fn add_dynamic_window(&mut self, title: String, rect: Rect, root: Object) -> Result<String>; // 返回 windowId handle
  #[napi] pub fn apply_tree_ops(&mut self, window_id: String, ops: Vec<Object>) -> Result<bool>;
  #[napi] pub fn step(&mut self) -> Result<bool>;                  // 非阻塞推进一帧
  #[napi] pub fn drain_callbacks(&mut self) -> Result<Vec<CallbackInvocationJs>>;
  #[napi] pub fn alloc_callback(&mut self) -> String;              // 为事件 prop 申请 CallbackId handle
  #[napi] pub fn get_property / set_property / close_window / focus_window
        / move_window / resize_window / list_windows / set_title / send_event
        / snapshot / schemas (...);
}
```
- 构造时调 `atto_ui_components::register_all_components()` 完成内置组件注册。
- 验收：headless 冒烟——JS 建窗口→`applyTreeOps` 改 text→`step`→`snapshot()` 断言文本。

**B.2 serde 转换(`convert.rs`，§4.2)**
- `Object`↔`serde_json::Value`↔`ComponentSpec`/`ComponentSpecChild`/`LayoutSpec`/`TreeOp`/`ComponentValue`/`CallbackInvocation`。
- TreeOp JS 形态：discriminated union(`{op:"set_prop", id, name, value}` 等，§6.2)。
- 验收：各类型 round-trip 单测；每种 TreeOp、每个 ComponentValue 分支解析正确。

**B.3 id handle 包装(`ids.rs`，§10.5)**
- `CallbackId`/`WindowId`(u64) ↔ 不透明 **string handle**，内部 Map 双向解析。
- **动机**：napi 把 u64 映射为 JS `BigInt`；string handle 规避 BigInt 人体工学与精度风险，JS 只做相等/查表。
- 验收：handle 双向解析一致；JS 侧从不做算术。

**B.4 错误映射(`error.rs`)**：`TreeError`/`anyhow::Error`→`napi::Error`，信息透传到 JS throw。

### 1.3 `@atto-ui/core`(TS 低层包，不依赖 react)

**L.1 native 加载 + 类型**：加载平台 `.node`，re-export napi 自动生成的 `.d.ts`。
**L.2 命令式构造器(§6.4)**：`VStack(...)`/`Text(...)`/`Button(...)` 等薄包装 spec 对象，供不使用 React 的低层用法 + 作为 React 库的底层。

### 1.4 `@atto-ui/react`(React 风格库)

> 核心新工作。基于 `react-reconciler` 自定义 HostConfig(§10.2)，起步 **LegacyRoot 同步模式**。

**U.1 HostConfig + host 模型(`reconciler.ts` / `host.ts`，§10.4)**
- host instance：`{ id, type, props, children, windowId }`；`createInstance` 时自增计数器→string，写入 `ComponentSpec.id`(必做，§10.4)。
- children 顺序镜像用于 anchor/index 换算(配合 R.1 可直接传 anchor id)。
- 方法→动作映射：

  | HostConfig | 动作 |
  |---|---|
  | `createInstance` / `createTextInstance` | 建 instance(后者→`TextSpan`) |
  | `appendChild` / `insertBefore` | `InsertBefore`(已挂载节点→Rust 侧等价 Move) |
  | `removeChild` | `Remove{id}` |
  | `prepareUpdate` / `commitUpdate` | props diff → 批 `SetProp` + 事件 `BindEvent`/`ClearEvent` |
  | `commitTextUpdate` | `TextSpan` 的 `SetProp text` |
  | `clearContainer` | `SetTree(空)` 或批量 `Remove` |
- op 累积：commit 期间 push 进 buffer，`resetAfterCommit` 按 `windowId` 分桶 flush。
- 验收：单测断言各操作产出的 `TreeOp` 序列(见 T.1)。

**U.2 `render()` + tick 主循环(`render.ts`，§10.9)**
- `render(element, { cols?, rows?, singleWindow? })`：建 `AppHost`(tickRate=0)→`createDesktopContainer`→`createContainer(LegacyRoot)`→`updateContainer`→启动 tick 微循环。
- 微循环：`setImmediate(tick)`；每 tick `step()`(非阻塞)→`drainCallbacks` 分发→React flush→op flush。
- 退出(Ctrl+Q→`step` 返回 false)→cleanup 恢复终端。
- 验收：PTY 启动/退出干净；LLM 流式 `for await` 灌 `setState`，UI 不阻塞(§5.2)。

**U.3 事件分发桥(`events.ts`，§10.8)**
- `callbackId → 最新 handler` Map：`callbackId` 绑定一次、Map 始终指向最新闭包，仅事件 prop 增删时 `BindEvent`/`ClearEvent`。
- 卸载时 `ClearEvent` + 回收 callbackId，防泄漏/stale handler。
- 验收：单测——handler 不重复 bind；卸载后不再触发。

**U.4 Window 映射(`desktop.ts`，§10.6)**
- 虚拟 `DesktopContainer`：只接受 `<Window>`/`<MenuBar>`/`<StatusBar>` 作直接子节点。
  - `appendChildToContainer(desktop, <Window>)`→`addDynamicWindow`，存 `windowId` 进 instance；`removeChildFromContainer`→`closeWindow`。
  - `<MenuBar>`/`<StatusBar>`→命令式 set 到固定槽位；普通组件挂 root→TS 类型禁止 + 运行期报错。
- `<Window title rect>`：props 改→`move`/`resize`/`setTitle`。
- op 路由：instance 从最近 `<Window>` 祖先继承 `windowId`，flush 按 window 分桶。
- `singleWindow:true`：自动包全屏 `<Window>`，单窗口 app 免写 Window。
- (可选)Portal：`createPortal(children, windowContainer)` 供全局 toast/modal。
- 验收：PTY 开/关窗口；单测两窗口 op 各归各位；Context 跨窗口贯通。

**U.5 文本组件(`text.ts`，§10.7)**
- `createTextInstance`→`TextSpan`；内联组件 `<Text>`/`<B>`/`<I>`/`<U>`/`<S>`/`<Link href>`→设置子 `TextSpan` style flags。
- `<Text>` 作 `RichText` 容器(多文本/内联子节点→多 `TextSpan`，Rust 侧合并)。
- `<Link href onClick>`→绑 `link` 事件，payload=url。
- `<Markdown>{md}</Markdown>`→`MarkdownViewer`(props `markdown`)。
- 过渡选项：U.5 前可先在 JS 侧拍平成 inline-markdown 串喂 `StyledLabel`(零 runtime 改动)，链路通后切结构化方案。
- 验收：快照 `<B>` 粗体、`<Text>hi {name}</Text>`、块级 markdown；PTY 点击链接。

**U.6 host 组件库 + JSX 类型(`components.ts` / `jsx.d.ts`)**
- wrapper：`<Button onClick>`/`<TextBox value onChange>`/`<ListBox>`/`<Table>`/`<VStack>`/`<HStack>`/`<Grid>`。
- 统一事件 prop 约定(`onClick`/`onChange`/`onSelect`)→ atto-ui 事件名映射。
- intrinsic elements JSX 命名空间；在 napi 生成的 native `.d.ts` 上扩展组件 props 类型。
- 受控输入回环验证：`<TextBox value onChange>`，确认外部 `SetProp value` + 变更事件不打架(§10.7 风险)。
- 验收：`tsc` 通过；PTY 受控输入正确。

### 1.5 测试与示例

**T.1 reconciler 单测(纯 JS，不进 native)**：mount/update/增删/重排/事件 bind-clear → 断言 `TreeOp` 序列。
**T.2 PTY 端到端**：复用 `crates/atto-ui-test-host`；计数器、表单(受控)、列表增删、多窗口。
**T.3 示例**：计数器、待办表单、**流式聊天**(Anthropic/OpenAI SDK 灌 token，验证 §5.2 共存)。

### 1.6 打包分发

**P.1 平台矩阵**：`@napi-rs/cli` 交叉编译 darwin-arm64/x64、linux-x64-gnu、win32-x64-msvc。
**P.2 npm 包**：主包 + `optionalDependencies` 指向各平台二进制包(§7.2)。
**P.3 CI**：tag→交叉编译→发布；Bun/Deno 兼容性实测(N-API 理论兼容，验 raw-mode 行为，§11)。

---

## 2. 里程碑排期

> 关键路径：B.0 → B.1/B.2/B.3 → U.1 → U.2/U.3 → U.4 → 测试 → 打包。
> **MVP 切片**：B.0+B.1+B.2+B.3 + U.1+U.2+U.3 = 单窗口 React 计数器跑通(文本先用过渡方案)，作为第一个可演示节点。

- **M0 脚手架**
  - B.0：crate + napi build + workspace 注册 + 冒烟。

- **M1 binding 核心(命令式 API 可用)**
  - B.1 / B.2 / B.3 / B.4：AppHost 全方法 + serde 转换 + id handle + 错误映射 + headless 冒烟。
  - L.1：`@atto-ui/core` 加载 native。
  - 可与 **R.1 / R.2** 并行(不同 crate)。

- **M2 runtime 改动**
  - R.1 `TreeOp::InsertBefore`；R.2 `RichText`/`TextSpan`。
  - (R.1 是否进 MVP 见 §3 待决；不进则 U.1 先用 index 版)。

- **M3 reconciler MVP**
  - U.1：HostConfig + 节点 id + props/事件 + 单窗口基础组件;静态树渲染→`useState` 改 text→子节点增删。

- **M4 主循环 + 事件桥**
  - U.2 `render()` + tick 微循环;U.3 `callbackId→handler` 分发。
  - 闭环验收:点击 Button→`onClick`→`setState`→屏幕更新;LLM 流式共存。

- **M5 文本子系统**
  - U.5(依赖 R.2)：`<Text>/<B>/<I>/<Link>`→`TextSpan`、`<Markdown>`。

- **M6 Window 映射**
  - U.4：虚拟 `DesktopContainer`、`<Window>` host 节点、op 分桶、`singleWindow`、(可选)Portal、跨窗口 Context。

- **M7 组件库 + TS**
  - U.6 / L.2：内置组件 React 封装、JSX 类型、受控输入、低层构造器。

- **M8 测试 + 示例**
  - T.1 reconciler 单测矩阵;T.2 PTY 端到端;T.3 计数器/表单/流式聊天示例;性能 sanity。

- **M9 打包分发**
  - P.1 / P.2 / P.3：跨平台预编译、npm 包、CI、Bun/Deno 冒烟、README/API 文档。

---

## 3. 关键设计取舍

- **react-reconciler 而非自研(§10.2)**：免费复用 hooks/调度/diff 与非 UI 的 React 组件;代价是吃透 HostConfig，起步用 LegacyRoot 同步模式更可控。
- **binding 先行、两条腿(§0.1)**：命令式 spec/op 层独立可用并自带价值，React 库在其上叠加;MVP 不必等全部组件就位。
- **单一虚拟 `DesktopContainer`(§10.6)**：保留 Desktop「非 spec 树、window 高频增删」的设计，又满足 React 单一 root + 跨窗口 Context;window 增删走命令式 add/close，不进 TreeOp。
- **id 全用 string(§10.5)**：节点 id 由 reconciler 生成;`CallbackId`/`WindowId` 在 binding 层包装成 string handle，规避 BigInt/精度。
- **文本结构化 + Rust 侧合并(§10.7)**：复用 `styled_text.rs` 渲染管线;`TextSpan` 用结构化 flags 避免 markdown 转义坑;`RichText` 在 Rust 侧合并相邻片段。
- **std-only + 不反向回调(§2、§5.3)**：async 交给 Node 事件循环，UI 事件轮询 drain，规避跨线程回调 JS 的重入难题。

---

## 4. 跟踪

- 每个能力点(R/B/L/U/T/P)：实现 + 测试 + 文档，完成后在对应 PR 注明编号。
- 设计依据回链 `NODE_BINDING.md` 对应小节(§N)；若实施中推翻设计，同步更新 `NODE_BINDING.md`。
- 与 `crates/atto-ui-python` 联动：B.1 暴露的 AppHost 能力应与 Python 侧保持对称，新增能力两处同步。
- 待决策(§3 / `NODE_BINDING.md` §11)：`InsertBefore` 是否进 MVP、文本是否走过渡方案、`stepDrainInput()`/限频重绘是否进首版、npm 包命名与 scope。
