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
| `addDynamicWindow` | `(title: string, rect: Rect, root: ComponentSpec) => number` | 添加动态窗口，返回 windowId |
| `applyTreeOps` | `(windowId: number, ops: TreeOp[]) => boolean` | 应用增量树操作 |
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
| `ComponentSpec` | `{ type: string; id?: string; props?: Record<string, ComponentValue>; events?: Record<string, number>; children?: ComponentSpecChild[] }` |
| `TreeOp` | discriminated union（`{ op: "set_prop", id, name, value }` 等） |
| `CallbackInvocation` | `{ callbackId: number; targetId: string \| null; event: string; payload: ComponentValue \| null }` |

命名约定：JS 侧用 camelCase，napi-rs 自动从 Rust snake_case 转换；`.d.ts` 自动生成。

### 6.3 回调模型

UI 事件不直接回调 JS 函数，而是经 `CallbackRegistry` 收集，由 JS 在每个 tick 通过 `drainCallbacks()` 拉取。JS 侧维护 `callbackId → handler` 映射进行分发。这与 Python binding 完全一致，保证两端语义统一。

### 6.4 高层封装（可选，TS 侧）

参考 Python `__init__.py` 的构造助手，提供 TS 侧便捷构造器（纯 JS/TS，不入 native）：

```ts
import { VStack, Text, Button, ChatMessageList } from "atto-ui";
const root = VStack({ padding: 1 }, [
  Text("Hello"),
  Button({ id: "send", text: "Send", onClick: 1 }),
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

## 10. 开放问题

- 高层 TS 封装的范围：仅薄包装 spec，还是提供更声明式的 JSX/模板式 API？
- 是否需要 `stepDrainInput()` 与限频重绘作为首版 API，还是留待性能验证后再加。
- npm 包命名与 scope（`atto-ui` vs `@atto-ui/*`）与现有 Python 包 `atto-ui` 的协调。
- Bun/Deno 兼容性的验证范围（N-API 理论兼容，需实测 raw-mode 终端行为）。
</content>
