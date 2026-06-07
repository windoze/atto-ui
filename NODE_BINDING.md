# Atto UI Node.js Binding 设计文档

> 目标：为 atto-ui 提供 Node.js 绑定，使其可被 Node 生态（尤其是最完整的 LLM SDK 生态：Anthropic SDK / OpenAI SDK / Vercel AI SDK 等）直接驱动，构建 agent 式终端 UI。
> 本文档只描述**设计**，不含里程碑与任务列表。

---

## 1. 背景与动机

当前 LLM 工具链生态最完整的运行时是 Node.js。若 atto-ui 能被 Node 直接驱动，则可让 JS/TS 编写的 agent 复用本库的多窗口 TUI、聊天列表、流式渲染、工具调用块等能力，发挥更大作用。

atto-ui 已有一套**语言无关的动态组件树 + 轮询式宿主驱动**模型，并已用 Python binding（`crates/atto-ui-python`，基于 pyo3）验证。Node binding 是该模型的兄弟实现，无需改动核心架构。

---

## 2. 设计原则

- **复用现有运行时层**：不为 Node 新增核心能力，所有交互走 `atto-ui::runtime` 的 `ComponentSpec` / `TreeOp` / `ComponentValue` 与 `AppHost`。
- **核心 crate 保持 std-only**：Node 自带事件循环与 async 运行时，binding 层不引入 tokio；核心 crate 永不依赖 tokio 的铁律不变。
- **轮询模型，不反向回调宿主**：宿主（Node）驱动 `step()` 并 `drainCallbacks()` 拉取事件，避免从任意 Rust 线程回调 JS 的重入/线程安全难题。
- **与 Python binding 对称**：两者共享同一套 Rust AppHost API 与数据模型，能力演进一处落地、两处受益。
- **保持 `#![forbid(unsafe_code)]`**（napi-rs 宏展开产生的 unsafe 用 `#![allow(unsafe_op_in_unsafe_fn)]` 局部豁免，与 Python crate 同策略）。

---

## 3. 架构定位

```
                 atto-ui (core, std-only)
                 └─ runtime: ComponentSpec / TreeOp / ComponentValue / AppHost
                        ▲                        ▲
            ┌───────────┘                        └───────────┐
   crates/atto-ui-python                          crates/atto-ui-node
   (pyo3, 轮询层)                                  (napi-rs, 轮询层)   ← 本文档
            │                                                  │
        Python agent                                      Node/TS agent
                                                     (Anthropic/OpenAI/AI SDK)

   atto-ui-async (tokio, feature-gated) —— 仅服务原生 Rust app，与两个 binding 无关
```

三条宿主路径（原生 Rust / Python / Node）互不污染：Python 与 Node 都是 std-only 轮询层，tokio 仅为原生 Rust 的可选项。

---

## 4. 技术选型

### 4.1 绑定框架：napi-rs

选用 [napi-rs](https://napi.rs)（N-API，pyo3 的等价物）：

- 产出 N-API 原生插件，**同时兼容 Node、Bun、Deno**。
- 自动从 Rust 代码生成 TypeScript 类型声明（`.d.ts`），优于 Python 侧手写 `.pyi`。
- 成熟的预编译二进制分发：`@napi-rs/cli` + npm 平台可选依赖（`@scope/pkg-darwin-arm64` 等），用户 `npm install` 即用，无需本地 Rust 工具链。
- 支持 `serde_json::Value` ↔ JS 值互转，可直接复用核心类型已有的 serde 派生。

被否选项：
- **Neon**：可用，但类型生成、预编译分发与多运行时兼容性弱于 napi-rs。
- **WASM**：无法直接做 raw-mode 终端 I/O（需要 tty/ioctl），不适合 TUI 宿主场景。

### 4.2 数据互转：走 serde

核心类型 `ComponentSpec` / `ComponentSpecChild` / `LayoutSpec` / `ComponentValue` / `TreeOp` / `CallbackInvocation` / `ComponentSchema` 均已 `derive(Serialize, Deserialize)`（见 `src/runtime/spec.rs`）。

因此 Node binding 的转换路径为：

```
JS object  ──(napi)──►  serde_json::Value  ──(serde)──►  ComponentSpec / TreeOp / ...
Rust 结果  ──(serde)──►  serde_json::Value  ──(napi)──►  JS object
```

相比 Python binding 手写的 dict→struct 解析（`py_to_component_spec` 等近千行），Node 侧用 serde 大幅减少样板，且与核心类型定义自动保持一致。

---

## 5. 事件循环集成（核心设计）

这是 Node binding 与 Python 的最大差异点，也是设计的关键。

### 5.1 问题

Node 是单线程事件循环。若同步死循环调用 `host.step()`，会阻塞事件循环，导致 timers、Promise、网络 IO（**包括 LLM 流式 HTTP/SSE**）全部停滞。

### 5.2 方案：非阻塞 step + JS 微循环

`AppHost::step()` 内部以 `event::poll(config.tick_rate)` 轮询输入（`src/app/run.rs`）。将 `tick_rate` 配置为 `Duration::ZERO`，`poll` 立即返回 → `step()` 非阻塞。

Node 侧用 `setImmediate` / `setInterval` 驱动微循环：

```js
const host = new AppHost();
host.addDynamicWindow("Chat", [0, 0, 80, 24], rootSpec);

function tick() {
  if (!host.step()) return;                 // 非阻塞：绘制 + 处理至多一个输入事件
  for (const ev of host.drainCallbacks()) {
    dispatch(ev);                            // 把 UI 回调分发到 JS 处理器
  }
  setImmediate(tick);                        // 让出事件循环，IO/Promise 得以推进
}
tick();
```

LLM 流式与 UI 更新天然组合（两者都在主 JS 线程，互不阻塞）：

```js
let acc = "";
for await (const delta of anthropic.messages.stream({ /* ... */ })) {
  acc += delta;
  host.applyTreeOps([
    { op: "set_prop", id: "assistant-msg", name: "text", value: acc },
  ]);
  // 下一个 tick 自动重绘
}
```

### 5.3 不采用线程化（ThreadsafeFunction）

napi-rs 提供 `ThreadsafeFunction` 可从 Rust 线程回调 JS，但：

- `AppHost` 持有终端句柄，非 `Send`，不能移入工作线程；终端 stdin/stdout 本身是进程级资源。
- 轮询模型已能在主线程优雅集成，无需引入线程与回调重入复杂度。

故初期**不使用**线程化方案，维持与 Python 一致的单线程轮询。

---

## 6. API 设计

### 6.1 `AppHost` 类（与 Python `PyAppHost` 对称）

| 方法 | 签名（TS 视角） | 说明 |
|---|---|---|
| 构造 | `new AppHost(config?)` | 初始化终端会话；默认 `tickRate=0`（非阻塞）、隐藏光标、开启鼠标捕获 |
| `addDynamicWindow` | `(title: string, rect: Rect, root: ComponentSpec) => string` | 添加动态窗口，返回不透明 windowId handle |
| `applyTreeOps` | `(windowId: string, ops: TreeOp[]) => boolean` | 应用增量树操作 |
| `step` | `() => boolean` | 非阻塞推进一帧；返回 false 表示退出 |
| `drainCallbacks` | `() => CallbackInvocation[]` | 取出自上次以来的 UI 回调事件 |
| `getProperty` | `(id: string, name: string) => ComponentValue` | 读取组件属性 |
| `schemas` | `() => ComponentSchema[]` | 列出已注册组件的 schema |

> 说明：上表为现有 Python 能力的 Node 对应。AppHost 在 B 部分（Python binding 完整化）补齐的能力 —— `sendEvent` / `closeWindow` / `focusWindow` / `moveWindow` / `resizeWindow` / `listWindows` / `setTitle` / `setProperty` / `snapshot` —— 因落在同一 Rust API，Node 可同步暴露，无需额外核心改动。

### 6.2 类型映射

| Rust | TS |
|---|---|
| `Rect` | `[number, number, number, number]` 或 `{ x, y, width, height }` |
| `ComponentValue` | `boolean \| number \| string \| string[] \| string[][] \| Uint8Array \| ComponentValue[] \| Record<string, ComponentValue> \| null` |
| `ComponentSpec` | `{ type: string; id?: string; props?: Record<string, ComponentValue>; events?: Record<string, string>; children?: ComponentSpecChild[] }` |
| `TreeOp` | discriminated union（`{ op: "set_prop", id, name, value }` 等） |
| `CallbackInvocation` | `{ callbackId: string; targetId: string \| null; event: string; payload: ComponentValue \| null }` |

命名约定：JS 侧用 camelCase，napi-rs 自动从 Rust snake_case 转换；`.d.ts` 自动生成。

### 6.3 回调模型

UI 事件不直接回调 JS 函数，而是经 `CallbackRegistry` 收集，由 JS 在每个 tick 通过 `drainCallbacks()` 拉取。JS 侧维护 `callbackId → handler` 映射进行分发。这与 Python binding 完全一致，保证两端语义统一。

### 6.4 高层封装（可选，TS 侧）

参考 Python `__init__.py` 的构造助手，提供 TS 侧便捷构造器（纯 JS/TS，不入 native）：

```ts
import { VStack, Text, Button, ChatMessageList } from "atto-ui";
const callbackId = appHost.allocCallback();
const root = VStack({ padding: 1 }, [
  Text("Hello"),
  Button({ id: "send", text: "Send", onClick: callbackId }),
]);
```

这些是对 6.2 spec 对象的薄包装，提升开发体验与类型安全，可后续逐步补全。

---

## 7. 包结构与分发

### 7.1 crate

新建 `crates/atto-ui-node`：

```toml
[package]
name = "atto-ui-node"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
napi = { version = "...", features = ["serde-json"] }
napi-derive = "..."
atto-ui = { path = "../.." }
atto-ui-components = { path = "../crates/atto-ui-components" }
serde_json = "..."

[build-dependencies]
napi-build = "..."
```

### 7.2 npm 包

```
atto-ui/                      # 主 npm 包（JS/TS 高层封装 + 加载 native）
├── index.js / index.d.ts     # 高层 API + 自动生成的 native 类型
├── package.json              # optionalDependencies 指向各平台二进制包
└── npm/
    ├── darwin-arm64/         # @atto-ui/core-darwin-arm64 (预编译 .node)
    ├── darwin-x64/
    ├── linux-x64-gnu/
    └── win32-x64-msvc/
```

由 `@napi-rs/cli` 在 CI 上交叉编译各平台 `.node` 并发布。用户 `npm install atto-ui` 时按平台拉取对应二进制，无需本地编译。

---

## 8. 风险与缓解

| 风险 | 说明 | 缓解 |
|---|---|---|
| stdin / raw mode 冲突 | crossterm 直接读 tty fd，需确保 Node `process.stdin` 不抢占 | Node 侧保持 `process.stdin` paused；由 crossterm 独占 raw mode |
| 信号处理 | SIGINT(Ctrl-C) / SIGWINCH(resize) 与 Node 协调 | resize 走 crossterm 事件；SIGINT 让 UI 先接管再决定退出 |
| 每 tick 重绘开销 | `step()` 每次都 `terminal.draw` | 后续加脏标记/限频，或用 ~16ms `setInterval` 替代 `setImmediate` |
| 单次 step 只读一个事件 | 输入积压时一帧消化一个 | 提供 `stepDrainInput()` 变体一次性 drain 所有待处理输入 |
| `AppHost` 非 Send | 持有终端句柄，须留在主线程 | 与 Python `unsendable` 同策略，本就单线程驱动；JS 侧文档约束不跨 worker 使用 |
| console 输出污染屏幕 | `console.log` 直写 stdout 会破坏 TUI | 文档约定日志走文件/UI 内日志面板；提供重定向辅助 |

---

## 9. 与原生 Rust / Python 路径的关系

- **不引入 tokio**：Node binding 的 async 完全由 Node 事件循环承担；`atto-ui-async`（tokio）仅服务原生 Rust app。
- **能力同源**：所有 AppHost 能力增强（事件注入、窗口管理、snapshot 等）在核心一处实现，Python 与 Node 两个 binding 同步暴露。
- **流式复用**：会话流式（`ChatMessageStore` 增量追加）等 chat 能力对 Node 透明可用 —— Node 经 `applyTreeOps`（或将来 chat 暴露的便捷方法）把 LLM token 持续灌入消息。

---

## 10. React 风格 UI 库设计

在第 1-9 节的 binding 之上，再构建一层 **React 风格的声明式 UI 库**，让用户用 JSX + hooks 编写终端 UI，而非手写 `ComponentSpec` / `TreeOp`。

### 10.1 目标与定位

- 用户编写标准 React 组件（JSX + `useState`/`useEffect`/`useContext`/Context/Suspense），由本库翻译为对底层窗口树的增量操作。
- **复用 React 生态**：不仅复用 React 的调度/diff/hooks 实现，还能直接复用大量**与渲染宿主无关**的 React 组件与库（状态管理、数据获取、纯逻辑组件、Context provider 等）。
- 与 binding 层解耦：UI 库完全建立在第 6 节的 `AppHost` API 之上，不要求核心做大改。

### 10.2 选型：`react-reconciler`（自定义 HostConfig）

采用官方 [`react-reconciler`](https://www.npmjs.com/package/react-reconciler) 包，自行实现一个 HostConfig（与 Ink、react-three-fiber 同路线），而非自研轻量 reconciler。

- **收益**：`useState`/`useEffect`/`useContext`/`useMemo`/Suspense/并发调度全部免费获得——这恰是"React 风格"里最难且最易出 bug 的部分；同时天然支持复用非 UI 的 React 组件（见 10.1）。
- **代价**：需吃透 HostConfig（约 30 个方法），其文档少、版本间有变动。
- **被否**：自研轻量 reconciler 需重写 diff + hooks，工作量更大、坑更多，唯一理由是想零 React 依赖，性价比低。
- **模式**：起步用 **LegacyRoot（同步模式）**，终端 UI 更可控；后续再评估并发模式。

> 关键认知：React Context / state 共享是 **fiber 树层面**的，与 host 节点如何物理切分无关。只要所有内容处于同一棵 fiber 树（单一 `createContainer`），跨窗口共享状态天然成立；多 `createContainer` 才会割裂 Context。

### 10.3 分层

```
用户代码: <Button onClick={...}/> + hooks            ← 使用方（JSX/TSX）
        ↓
react + react-reconciler  (调度 / hooks / diff)       ← 直接复用 npm 包
        ↓
自定义 HostConfig                                     ← React 库核心
   把 mount/update/unmount/reorder 翻译成 TreeOp[]
        ↓
Node binding: AppHost (napi-rs)                       ← 第 1-9 节
   applyTreeOps / drainCallbacks / step / addDynamicWindow
        ↓
atto-ui runtime (Rust): ComponentTree / TreeOp        ← 已存在
```

`react-reconciler` 在 commit 阶段产出的就是"增量变更"，与 atto-ui 的 `TreeOp`（`Insert`/`Remove`/`Replace`/`Move`/`SetProp`/`ClearProp`/`BindEvent`/`ClearEvent`）语义同构——这是本方案最契合的基础。

### 10.4 节点 id 与 TreeOp 对齐

除 `SetTree` 外，所有 `TreeOp` 都靠 `id` / `parent_id` 定位节点（见 `src/runtime/spec.rs` 的 `apply_tree_op`）。而 React host 节点没有天然 id，因此：

- **reconciler 必须在 `createInstance` 时为每个节点分配稳定 id，写入 `ComponentSpec.id`**（自增计数器 → 字符串即可）。这是必做项，也顺带解决了 64 位问题（见 10.5）。

React（mutation 模式）commit 阶段调用的 host 方法与 `TreeOp` 的映射：

| React host 调用 | TreeOp | 备注 |
|---|---|---|
| `appendChild` / `appendInitialChild` | `Insert { index: len }` | `apply_tree_op` 里 `idx.min(len)` 会钳制 |
| `removeChild` | `Remove { id }` | React 给节点引用，取其 id |
| `commitUpdate`（props diff） | 一批 `SetProp`/`ClearProp` + 事件变化 `BindEvent`/`ClearEvent` | |
| `commitTextUpdate` | `SetProp text`（或 TextSpan，见 10.7） | |
| `clearContainer` | `SetTree(空)` 或批量 `Remove` | |

**两个需要适配的 gap：**

1. **index vs 锚点**：`Insert`/`Move` 用数字 index，而 React 的 `insertBefore(parent, child, beforeChild)` 给的是**锚点节点引用**。reconciler 需自行维护每个 parent 的有序 children 镜像以换算 index；批量重排时 index 会随中间状态漂移，逐个计算易错。
2. **move 判定**：React 重排时对**已挂载**节点再次调用 `insertBefore`（类似 DOM 的"自动 detach 再插入"），但不告知这是新建还是移动。host 须自行判断 child 是否已在树中：已在 → `Move`，否则 → `Insert`。`apply_tree_op` 的 `Move` 已含"不能移进自身子树"的保护。

**建议的 runtime 小改动（可选但推荐）**：新增一个锚点版插入变体，把 index 计算与 move 判定的负担从 JS 移到 Rust，并规避批内 index 漂移：

```rust
// src/runtime/spec.rs TreeOp
InsertBefore { parent_id: String, anchor_id: Option<String>, child: ComponentSpecChild }
// anchor=None 即 append；若 child 已存在则等价于 Move
```

起步可先用现有 index 版（由 reconciler 维护顺序镜像），若重排实现易错再引入锚点版。

### 10.5 64 位 id 的跨语言传递

三类 id 需要区分：

- **节点 id 是 `String`**（`ComponentSpec.id: Option<String>`），由 reconciler 自己生成 → **无 64 位问题**。
- 真正是 64 位的只有 **`CallbackId(u64)`** 与 **`WindowId(u64)`**（u64 newtype，经 `raw()`/`from_raw()` 出入）。

注意 **napi-rs 把 Rust `u64`/`i64` 映射为 JS `BigInt` 而非 `number`**（Python 侧 pyo3 映射为无限精度 int，故未暴露此问题）。处理策略：

- **推荐**：在 binding 层把 `CallbackId` / `WindowId` 包装为**不透明 string handle** 暴露给 JS，JS 仅做相等比较与查表（如 `callbackId → handler` 的 Map），从不做算术。既规避 BigInt 人体工学问题，又彻底规避精度风险，并与节点 id 的 string 风格统一。
- 次选：直接暴露 BigInt（napi 默认），精确但 JS 侧啰嗦。

> 实务上一个 TUI 永远分配不到 2^53 个回调/窗口，数值始终在安全范围；string 包装是为了类型一致与稳健，而非数值真会溢出。

### 10.6 Window 映射：单一虚拟 `DesktopContainer`

**约束**：`Desktop` 刻意**不是一棵 ComponentSpec 树**——window 是高频增删对象，组织成大树需频繁整树操作且无收益；且 Desktop 只容纳固定的单个 MenuBar/StatusBar，其余子对象全是 window。而 React 预设单一 root container。需要协调两者。

基于 10.2 的关键认知（Context 共享在 fiber 层，不要求 host 物理连续树），协调方案为：

```
createContainer(DesktopContainer)         // 单一 React root，Context/state 全树贯通
        │  (虚拟容器，本身不是 spec 树)
   <App> 只能产出这几类顶层 host 节点：
        ├── <MenuBar/>      (0..1，命令式设到 desktop 固定槽位)
        ├── <StatusBar/>    (0..1，同上)
        └── <Window/> * N   (高频增删)
                └── 内部常规组件树 → 路由到该 window 的 spec 树
```

- **`render(<App/>)` → 单一 `createContainer(desktopContainer)`**：满足 React 心智，Context / 主题 / 跨窗口 state 天然贯通。
- **`DesktopContainer` 是虚拟容器，只接受特定子节点**：
  - `appendChildToContainer(desktop, <Window>)` → 命令式 `addDynamicWindow()`，把返回的 `windowId` 存入该 Window 的 host instance。
  - `removeChildFromContainer(desktop, <Window>)` → `closeWindow(windowId)`。
  - `<MenuBar>` / `<StatusBar>` → set 到 desktop 固定槽位。
  - 普通组件直接挂 root → 用 TypeScript 在编译期禁止（`<App>` 合法直接子节点仅这三类），运行期兜底报错。
- **`<Window>` 内部的常规组件树**：host 将其增删改路由到**该 window 的 spec 树**（`applyTreeOps(windowId, ops)`）。每个 host instance 从最近的 `<Window>` 祖先继承 `windowId`。
- **window 频繁增删 = desktop 根下 N 棵独立 host 子树的挂卸**，互不影响、不构成大 spec 树，正合 atto-ui 设计意图。desktop 级操作（增删窗口、设菜单栏）走命令式 binding，**不进 TreeOp**。

**op 按 window 分桶**：reconciler 在 `resetAfterCommit` flush 时，把累积的 op 按目标 `windowId` 分组，对每个 window 调一次 `applyTreeOps(windowId, ops)`；desktop 级变更单独走命令式调用。（`Desktop::apply_tree_ops` 已是 per-window，基础具备。）

**便利补充**：

- **单窗口便捷模式**：`render(<App/>, { singleWindow: true })` 自动包一层全屏 `<Window>`，开发者直接写 UI、无需关心 window 概念（覆盖大多数 app）。
- **Portal 作为可选项**：上述方案中 `<Window>` 是正常父子关系，不需要 portal。仅当想"在 React 树的非 Window 位置定义某窗口内容"（如全局 `<Toast>`/`<Modal>`）时，再用 `createPortal(children, windowContainer)` 投射——同一 fiber 树，Context 照样贯通。起步可不做。

### 10.7 文本子系统

库内已有**三档**文本渲染能力：

| 组件 | 能力 | 文本来源 | 备注 |
|---|---|---|---|
| `Text`（runtime） | 纯文本 | `text` prop | `allow_children(false)` |
| `StyledLabel`（runtime） | 单行 + 内联样式 `**粗** *斜* __下划线__ ~~删除~~ [文字](url)` | `text` prop | 有 `link` 事件，payload 为 url string；`allow_children(false)` |
| `MarkdownViewer`（`crates/atto-ui-markdown`） | 完整块级 markdown，可滚动 | `markdown` prop | 经 `register_runtime_components()` 注册到全局 |

底层 `src/text/styled_text.rs` 将文本拆为 `StyledTextSegment { text, style(bold/italic/underline/strike), link_url }`，再以 `spans_from_segments` / `slice_segments` / `hit_test_link` 渲染。**这套 segment 渲染管线是可复用的核心资产**，目前其输入是 `parse_inline(markdown字符串)`。

**设计（文本节点保留，合并在 Rust 侧）**：现有 `Text`/`StyledLabel` 均 `allow_children(false)`、文本走 prop，因此需一项 **runtime 扩展**：

1. **新增富文本容器 + 文本片段节点**（runtime）：
   - `RichText`（`allow_children(true)`）接受 `TextSpan` 子节点。
   - `TextSpan` 的 props 用**结构化 style flags**（`bold`/`italic`/`underline`/`strike`/`color`/`href`），而非 markdown 标记串——避免用户文本中的 `*`、`[` 被误解析/需转义。
   - build 时遍历子节点直接构造 `Vec<StyledTextSegment>`，复用现有 `spans_from_segments` 渲染（仅替换 segment 来源，渲染管线原样复用）。
2. **reconciler 侧**（JS）：
   - `createTextInstance(text)` → 一个 `TextSpan` 节点。
   - 内联样式组件 `<Text>` / `<B>` / `<I>` / `<U>` / `<S>` / `<Link href>` → 设置子 `TextSpan` 的 style flags。
   - 相邻文本片段的合并、空 span 清理交给 Rust 侧 `RichText` build（"合并在 binding 侧"），Rust 侧还能做单 span 增量更新与链接命中测试。
3. **块级富文本**：`<Markdown>{md}</Markdown>` → 直接映射 `MarkdownViewer`（props `markdown`），无需 reconciler 特殊处理。
4. `link` 事件 → `onLinkClick`，payload 为 url string。

> 快速起步可先在 JS 侧把 `<Text bold>` 子树拍平成 inline-markdown 串喂给现有 `StyledLabel`（零 runtime 改动），基础链路通后再升级到结构化 `RichText`/`TextSpan`。建议直接做结构化方案，长期更干净。

### 10.8 事件分发桥

- JS 侧维护 `callbackId → 最新 handler` 的 Map。
- 主循环每帧 `drainCallbacks()` → 查表 → 调 handler → handler 内 `setState` → React 调度重渲染 → 新 `TreeOp[]` flush。
- **优化**：`callbackId` 绑定一次，Map 始终指向最新闭包，避免每次 render 都重新 `BindEvent`（仅在事件 prop 增删时才 Bind/Clear）。
- 组件卸载时 `ClearEvent` 并回收 `callbackId`，防止泄漏与 stale handler。

### 10.9 主循环整合

- 提供 `render(element, { cols, rows, singleWindow? })` 入口（类比 Ink 的 `render`）。
- 用 `setImmediate` / 定时器驱动第 5 节的非阻塞 `host.step()`；每 tick：`step()` → `drainCallbacks` 派发 → 让 React flush。
- 处理退出（如 Ctrl+Q）、cleanup、恢复终端。
- 与 LLM 流式等 IO 共存于主 JS 线程，互不阻塞（见第 5 节）。

### 10.10 需要的核心改动小结

React 库带来的净增改动很少，且都不大：

- **runtime**：
  - （推荐）新增锚点版 `InsertBefore` 变体（10.4）。
  - 新增 `RichText` + `TextSpan` 组件，复用 `styled_text.rs` 的 segment 渲染（10.7）。
- **binding**：
  - 暴露 `allocCallback()`（供 JS 为事件 prop 申请 `CallbackId`）。
  - 将 `CallbackId` / `WindowId` 包装为 string handle（10.5）。
  - 暴露 desktop 级 window 增删，并支持 op 按 `windowId` 路由（10.6）。
- **其余**（节点 id 分配、children 顺序镜像、move 判定、事件分发、文本片段构造）均在 reconciler / JS 侧实现，不触及核心。

---

## 11. 开放问题

- 高层 API 形态已定为 **React 风格（react-reconciler + JSX）**（见第 10 节）；薄包装 spec 的构造器（6.4）可作为不依赖 React 的低层入口并存。
- 锚点版 `InsertBefore`（10.4）是首版就引入，还是先用 index 版跑通再迭代。
- 是否需要 `stepDrainInput()` 与限频重绘作为首版 API，还是留待性能验证后再加。
- 文本子系统首版走结构化 `RichText`/`TextSpan`，还是先用拍平成 inline-markdown 的过渡方案（10.7）。
- npm 包命名与 scope（`atto-ui` vs `@atto-ui/*`）与现有 Python 包 `atto-ui` 的协调。
- Bun/Deno 兼容性的验证范围（N-API 理论兼容，需实测 raw-mode 终端行为）。
</content>
