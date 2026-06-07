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

### [DONE] NR1 — 审阅 NT1
- 确认 crate-type/feature/依赖正确，workspace 不引入 tokio。
- 确认 `.node` 产物可被 Node require。
- 运行 `cargo build --workspace` + JS 冒烟。
**完成记录（2026-06-07）**：
- 审阅 `crates/atto-ui-node` 脚手架：`Cargo.toml` 使用 `cdylib`，启用 `napi` 的 `serde-json` feature，包含 `napi-derive`、`atto-ui`、`atto-ui-components`、`serde_json` 和 `napi-build`；根 workspace 已包含该 crate。
- 检查 Node 入口与类型文件：生成的 `index.js`/`index.d.ts` 导出 `version()`，JS 冒烟通过 package 入口 require native `.node`。
- 检查 Node crate 依赖图未包含 `tokio`；workspace 既有 `atto-ui-async` 仍仅以可选 feature 声明 tokio，NT1 未新增核心/Node tokio 依赖。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo build --workspace`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`；`node __test__/version.cjs`；`cargo test --all --all-targets`。

### [DONE] NT2 — serde 数据转换层（B.2）
**文件**：`crates/atto-ui-node/src/convert.rs`
**现状**：核心类型 `ComponentSpec`/`ComponentSpecChild`/`LayoutSpec`/`ComponentValue`/`TreeOp`/`CallbackInvocation`/`ComponentSchema` 均已 `derive(Serialize,Deserialize)`（`src/runtime/spec.rs`）。Python 侧是手写 dict→struct（近千行），Node 改走 serde。
**步骤**：
1. `Object`/`JsUnknown` ↔ `serde_json::Value` ↔ 上述类型（`napi` 的 `serde-json` 支持）。
2. 约定 JS 侧 `TreeOp` 形态为 discriminated union（`{op:"set_prop",id,name,value}` 等，§6.2）；snake_case 与核心枚举对齐。
3. `ComponentValue` 全分支：bool/number/string/string[]/string[][]/bytes/list/map（§6.2 类型映射）。
**测试**：Rust 单测——`ComponentSpec`/`TreeOp`/`ComponentValue` round-trip；每种 op 解析正确。
**验收**：JS 对象与核心类型双向转换正确，且转换代码量显著小于 Python 手写路径。
**完成记录（2026-06-07）**：
- 新增 `crates/atto-ui-node/src/convert.rs`，提供 napi `Unknown`/`Object` 与 `serde_json::Value` 的桥接函数，并实现 `ComponentValue`、`ComponentSpec`/`ComponentSpecChild`/`LayoutSpec`、`TreeOp`、`CallbackInvocation`、`ComponentSchema` 的双向转换。
- `TreeOp` 采用 JS discriminated union 形态并输出 snake_case `op`（`set_tree`/`set_prop`/`bind_event` 等）；单测覆盖现有全部 `TreeOp` 变体解析与 round-trip。
- `ComponentValue` 覆盖 null/bool/i64/u64/f64/string/string[]/string[][]/rect/bytes/list/map 分支；`ComponentSpec` 支持 JS `type` 字段、可选 props/events/children，以及 child layout/meta。
- 新增 Rust 单测覆盖组件值、组件树、树操作、callback invocation、schema 和错误上下文；转换层代码复用 serde/JSON 桥接，避免 Python binding 的大段手写 dict 解析路径。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-node`；`cargo test --all --all-targets`。

### [DONE] NR2 — 审阅 NT2
- 确认所有 `TreeOp` 变体、`ComponentValue` 分支均覆盖且与核心定义一致。
- 确认错误（缺字段/类型不符）有清晰报错而非 panic。
- 运行转换单测。
**完成记录（2026-06-07）**：
- 审阅 `crates/atto-ui-node/src/convert.rs` 与 `src/runtime/spec.rs`，确认现有 `TreeOp` 变体均有 discriminated union 解析与 round-trip 覆盖，且与当前核心定义一致。
- 修复 `ComponentValue` plain JSON 歧义：空 `StringList`/`Table`、只含字符串或字符串数组的 `List`、矩形形状或保留 `$type` 形状的 `Map` 现在通过 `$type` + `data` 逃逸格式保持稳定 round-trip；非歧义数组/对象仍保持普通 JS 形态。
- 补齐 `Rect` 的 `[x,y,width,height]` 输入解析，并扩展错误测试，确认缺字段/类型不符路径返回带上下文的 `napi::Error`，转换代码路径无 panic。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] NT3 — id handle 包装 + 错误映射（B.3 / B.4）
**文件**：`crates/atto-ui-node/src/ids.rs`、`src/error.rs`
**现状**：`CallbackId`/`WindowId` 是 u64 newtype；napi 把 u64 映射为 JS `BigInt`（§10.5）。节点 id 本身是 `String`（`ComponentSpec.id`），无需包装。
**步骤**：
1. `ids.rs`：`CallbackId`/`WindowId` ↔ 不透明 **string handle**，内部 Map 双向解析；JS 侧只做相等/查表。
2. `error.rs`：`TreeError`/`anyhow::Error` → `napi::Error`，信息透传到 JS throw。
**测试**：单测 handle 双向解析一致；错误转换保留消息。
**验收**：JS 侧不接触 BigInt、不做 id 算术；Rust 错误能在 JS 以 Error 抛出。
**完成记录（2026-06-07）**：
- 新增 `crates/atto-ui-node/src/ids.rs`，提供 `CallbackId`/`WindowId` 与不透明 string handle 的双向 Map；handle 使用独立命名空间，支持解析、复用与释放后失效。
- 新增 `crates/atto-ui-node/src/error.rs`，将 `TreeError` 与 `anyhow::Error` 转为 `napi::Error` 并保留 display 消息。
- 更新 `convert.rs` 中事件 callback id 的 JS 形态为 string handle，`ComponentSpec`/`TreeOp::BindEvent`/`CallbackInvocation` 不再接受数字 callback id，避免 JS 侧接触 BigInt 或做 id 算术。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] NR3 — 审阅 NT3
- 确认 handle 包装无泄漏（窗口/回调销毁后 handle 失效处理合理）。
- 确认错误信息不丢失、不暴露内部细节。
- 运行单测。
**完成记录（2026-06-07）**：
- 审阅 `crates/atto-ui-node/src/ids.rs`，确认 `CallbackId`/`WindowId` 使用独立 string handle 命名空间，JS 侧不接触 BigInt；补充 stale handle 回归测试，确保释放后旧 handle 失效，同一 runtime id 重新分配不会重新验证旧 handle。
- 审阅 `crates/atto-ui-node/src/convert.rs` 的 callback id 接线，确认 `ComponentSpec`/`TreeOp::BindEvent`/`CallbackInvocation` 均通过 `CallbackHandles` 解析与编码，数字 callback id 被拒绝并返回上下文错误。
- 修复 `crates/atto-ui-node/src/error.rs` 的 `anyhow::Error` 映射：JS `Error` reason 现在保留 display source chain（例如外层 context + 根因），不使用 debug/backtrace 细节；`TreeError` display 消息保持原样透传。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] NT4 — `#[napi] AppHost` 全方法暴露（B.1）
**文件**：`crates/atto-ui-node/src/lib.rs`
**现状**：Rust `::atto_ui::app::AppHost` 已具备 `new`/`new_headless`/`step`/`drain_callbacks`/`add_dynamic_window`/`apply_tree_ops`/`get_property`/`set_property`/`send_event`/窗口管理/`snapshot`/`schemas`（agent-ui 归档计划 T3–T5 落地）。
**步骤**：
1. 接线全部方法（签名见 PLAN-1 §1.2 B.1），构造时调 `atto_ui_components::register_all_components()`。
2. 构造默认 `tickRate=0`（非阻塞）、隐藏光标、鼠标捕获；支持 `headless` 选项。
3. 新增 `alloc_callback() -> String`（为事件 prop 申请 `CallbackId` handle，供 React 库 §10.8）。
4. 用 NT2/NT3 的转换与 handle 包装贯通参数与返回值。
**测试**：headless 冒烟（JS）——建窗口→`applyTreeOps` 改 text→`step`→`snapshot()` 断言文本；`drainCallbacks` 取回注入事件。
**验收**：JS 能用命令式 spec/op 驱动一个 headless 窗口，能力与 Python `PyAppHost` 对称。
**完成记录（2026-06-07）**：
- 在 `crates/atto-ui-node/src/lib.rs` 暴露 `#[napi] AppHost`：支持 constructor 配置（`headless`、`cols`/`rows`、`tickRate=0`、鼠标捕获、隐藏光标等）、`addDynamicWindow`、`applyTreeOps`、`step`、`drainCallbacks`、`allocCallback`、`sendEvent`、窗口管理、属性读写、`snapshot`、主题方法与 `schemas`。
- 复用 NT2/NT3 转换层与 string handle：窗口/回调 id 均以 `atto:window:*` / `atto:callback:*` 暴露；`CallbackInvocation`、snapshot 和 window list 输出不暴露 raw `u64`/BigInt。
- 新增 `event.rs` 支持 JS 注入 key/mouse/paste/resize/focus 事件；补充组件错误映射，保证属性读取等路径返回清晰 JS Error。
- 新增 `__test__/app_host.cjs`，覆盖 headless 建窗口、`applyTreeOps` 修改文本、`getProperty`/`snapshot` 断言、`sendEvent` 触发 Button callback、`drainCallbacks`、窗口列表/标题/关闭与 stale handle 失效。
- 更新 napi 生成的 `index.js`/`index.d.ts`，导出 `AppHost`、`AppHostConfig`、`Rect` 与 `registerAllRuntimeComponents()`。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-node`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`；`npm test`。

### [DONE] NR4 — 审阅 NT4
- 确认方法集与 Python 对称，签名与 §6.1 一致。
- 确认 `step()` 在 `tickRate=0` 下非阻塞；`drainCallbacks` 载荷（callbackId/targetId/event/payload）齐全。
- 确认 headless 路径不依赖真实 PTY。
- 运行 JS 冒烟 + 相关单测。
**完成记录（2026-06-07）**：
- 审阅 `crates/atto-ui-node/src/lib.rs` 与 `crates/atto-ui-python/src/lib.rs`，确认 Node `AppHost` 覆盖 B.1 要求的方法集：constructor/headless、`addDynamicWindow`、`applyTreeOps`、`step`、`drainCallbacks`、`allocCallback`、事件注入、窗口管理、属性读写、snapshot、theme 与 schemas；窗口/回调 id 均按 NT3 以 string handle 暴露。
- 修复 `addDynamicWindow` 的 `Rect` 输入兼容性：Node 侧现在与文档/Python 路径一致，支持 `{ x, y, width, height }` 和 `[x, y, width, height]` 两种形态；补充 Rust 单测与 JS headless 冒烟覆盖数组输入。
- 校正 `NODE_BINDING.md` §6.1/§6.2 中遗留的 numeric id 表述，明确 windowId/callbackId 为不透明 string handle；重新生成 `index.d.ts`，`addDynamicWindow` 类型为 `Rect | [number, number, number, number]`。
- 确认 `tickRate=0` 为 Node 默认配置，`step()` 在 headless 路径不进入 crossterm poll；JS 冒烟验证 `drainCallbacks` 返回完整 `{ callbackId, targetId, event, payload }` 载荷，headless 构造和 snapshot 均走内存路径。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test -p atto-ui-node`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`；`npm test`。

### [DONE] NT5 — `@atto-ui/core` native 加载（L.1）
**文件**：新增 `packages/core/`（`index.ts`、`native.d.ts`）
**现状**：尚无 npm 包；napi 可自动生成 `.d.ts`。
**步骤**：
1. 加载平台 `.node`，re-export napi 生成的类型声明。
2. 暴露 `AppHost` 类型与 `ComponentSpec`/`TreeOp`/`ComponentValue`/`CallbackInvocation` 的 TS 类型。
**测试**：`tsc --noEmit` 通过；JS 端 import 并跑通 NT4 的 headless 冒烟。
**验收**：`@atto-ui/core` 可作为 React 库（阶段三起）的底层依赖。
**完成记录（2026-06-07）**：
- 新增 `packages/core/`，包含 `package.json`、`tsconfig.json`、CommonJS 运行时入口 `index.js`、TS 类型入口 `index.ts`、native loader `native.js` 与 raw binding 声明 `native.d.ts`。
- `native.js` 支持 `ATTO_UI_NATIVE_LIBRARY_PATH` / `NAPI_RS_NATIVE_LIBRARY_PATH` 覆盖、本包平台 `.node`、后续 `@atto-ui/core-*` 平台包、现有 `@atto-ui/node-*` / `@atto-ui/node` 包，以及当前 workspace 的 `crates/atto-ui-node` fallback。
- `index.ts` 暴露强类型 `AppHost`、`ComponentSpec`、`ComponentSpecChild`、`LayoutSpec`、`ComponentValue`、`TreeOp`、`CallbackInvocation`、snapshot/window/schema/input event 等类型，避免把 napi 生成声明中的 `any` 泄漏给 `@atto-ui/core` 消费者。
- 新增 `packages/core/__test__/headless.cjs`，经 `@atto-ui/core` 导入 native binding，完成 headless 建窗口、`applyTreeOps` 改文本、snapshot、事件注入与 callback drain 冒烟；新增 `__test__/types.ts` 覆盖核心类型与返回值非 `any`。
- 验证通过：`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`node packages/core/__test__/headless.cjs`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc --noEmit`（`packages/core`）；`npm test`（`packages/core`）。

### [DONE] NR5 — 审阅 NT5
- 确认类型声明与 Rust 侧一致、无 any 泄漏。
- 确认跨平台 `.node` 加载路径正确（为 P 阶段分发铺垫）。
- 运行 `tsc` + 冒烟。
**完成记录（2026-06-07）**：
- 审阅 `packages/core/index.ts`、`native.d.ts` 与 `crates/atto-ui-node/src/lib.rs`/`convert.rs`，确认对外 `AppHost`、`ComponentSpec`、`TreeOp`、`ComponentValue`、`CallbackInvocation`、snapshot/window/schema/input event 类型与当前 Rust/Node JSON 形态一致。
- 确认 `packages/core/index.ts` 对外类型无 `any` 泄漏；raw native 声明仅保留 `unknown`/`object` 边界，并由 typed facade 收窄。
- 审阅 `packages/core/native.js` 加载顺序，确认支持显式环境变量覆盖、本包平台 `.node`、未来 `@atto-ui/core-*` 平台包、现有 `@atto-ui/node-*`/`@atto-ui/node` 以及 workspace `crates/atto-ui-node` fallback。
- 验证通过：`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`node packages/core/__test__/headless.cjs`；`npm test --prefix packages/core`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

---

## 阶段二：M2 runtime 改动（可与阶段一并行，改 `atto-ui` 核心）

### [DONE] NT6 — `TreeOp::InsertBefore` 锚点版插入（R.1）
**文件**：`src/runtime/spec.rs`、`src/runtime/tree.rs`
**现状**：现有 `Insert{parent_id,index,child}` 用数字 index；React `insertBefore` 给的是锚点节点引用，且重排对已挂载节点再次 `insertBefore`（§10.4 两个 gap）。
**步骤**：
1. `spec.rs`：`TreeOp` 新增 `InsertBefore { parent_id: String, anchor_id: Option<String>, child: ComponentSpecChild }`。
2. `apply_tree_op`：`anchor=None`→append；给定 anchor→解析为 index 插入；若 `child` id 已存在树中→等价 `Move`（先 detach）。
3. `tree.rs` `apply_ops_incremental`：为 `InsertBefore` 加增量分支（参照现有 `Insert`/`Move`），避免全量重建。
4. 向后兼容：旧 `Insert{index}` 与 Python 路径不变。
**测试**：单测覆盖 append / insert-before-anchor / 已存在节点→move 三态；增量路径不触发全量重建。
**验收**：三态行为正确；现有 runtime 测试与 Python 路径全绿。
**完成记录（2026-06-07）**：
- `src/runtime/spec.rs` 新增 `TreeOp::InsertBefore { parent_id, anchor_id, child }`，支持 `anchor_id=None` append、按父节点直接子锚点插入，以及 child id 已存在时先 detach 再按锚点插入的 move 语义；保留旧 `Insert { index }` 行为不变。
- 补齐移动保护：拒绝移动 root、拒绝移入自身或后代；锚点必须是目标父节点的直接子节点，移动到自身当前锚点位置按 no-op 处理。
- `src/runtime/tree.rs` 的 `apply_ops_incremental` 增加 `InsertBefore` 分支；新增插入/锚点移动 helper，增量插入和重排保持既有 `ComponentNode` id，测试覆盖不触发全量重建。
- Node 转换层与 `@atto-ui/core` 类型同步新增 `insert_before` discriminated union；JS headless 冒烟覆盖 anchor 插入、append 和已存在节点 move。
- 验证通过：`cargo fmt`；`cargo test -p atto-ui runtime::`；`cargo test -p atto-ui-node convert::tests::tree_op_parses_every_variant`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test --prefix packages/core`。

### [DONE] NR6 — 审阅 NT6
- 确认 anchor 解析、move 等价语义与"不能移进自身子树"保护正确。
- 确认增量分支不误判结构变更、不全量重建。
- 确认对现有 `Insert`/Python 零影响。
- 运行 runtime 单测 + 全 workspace。
**完成记录（2026-06-07）**：
- 审阅 `src/runtime/spec.rs` 的 `TreeOp::InsertBefore`：确认 `anchor_id=None` append、指定锚点按目标父节点直接子节点解析，已存在 child id 走 detach 后按锚点插入的 move 等价语义；移动 root、移入自身或后代的保护会在提交前失败并保持原树不变。
- 审阅 `src/runtime/tree.rs` 的增量分支：确认新插入与已存在节点重排均走增量更新，重排保留已有 `ComponentNode`，每步以 spec/view 形状校验保护，失败时回退重建且不把部分 view 更新留存。
- 确认旧 `Insert { index }` 分支未改变，Python binding 的既有 tree op 解析与测试保持通过；Node 转换层与 `@atto-ui/core` 类型已包含 `insert_before`，JS headless 冒烟覆盖 anchor、append 与 move。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test`（`packages/core`）。

### [DONE] NT7 — `RichText` + `TextSpan` 结构化富文本（R.2）
**文件**：`src/text/styled_text.rs`、`src/widgets/`（新增）、`src/runtime/builtins.rs`
**现状**：`Text`/`StyledLabel` 均 `allow_children(false)`、文本走 prop；`styled_text.rs` 的 `StyledTextSegment` 渲染管线（`spans_from_segments`/`slice_segments`/`hit_test_link`）目前仅由 `parse_inline(markdown串)` 喂入（§10.7）。
**步骤**：
1. `styled_text.rs`：为 `StyledTextSegment` 增加可由结构化字段直接构造的入口（当前 `pub(crate)`）。
2. `TextSpan` 组件：props 为结构化 flags（`text`/`bold`/`italic`/`underline`/`strike`/`color?`/`href?`），`allow_children(false)`；注册到 builtins。
3. `RichText` 容器：`allow_children(true)`，build 时遍历 `TextSpan` 子节点→`Vec<StyledTextSegment>`→`spans_from_segments` 渲染；相邻同 style 合并、空 span 清理。
4. `href` 命中复用 `hit_test_link`，发 `link` 事件（payload=url）。
**测试**：headless 快照——粗/斜/下划线/删除线/链接渲染正确；PTY——点击链接触发回调。
**验收**：富文本以结构化子节点驱动，复用既有 segment 渲染管线，无需 markdown 转义。
**完成记录（2026-06-07）**：
- `src/text/styled_text.rs` 新增结构化 `StyledTextSegment::structured` 与 `normalize_segments` 入口，支持相邻同 style 合并、空 span 清理和 `color` 前景色，同时继续复用 `spans_from_segments` / `slice_spans_from_segments` / `hit_test_link` 管线。
- 新增 `src/widgets/rich_text.rs`：`TextSpan` 暴露 `text`/`bold`/`italic`/`underline`/`strike`/`color`/`href` 结构化 props；`RichText` 接收 `TextSpan` 子节点，绘制时生成 segments，并复用链接命中逻辑发 `link` 事件（payload 为 url string）。
- 在 `src/runtime/builtins.rs`、`src/widgets/mod.rs`、`src/composable/mod.rs` 注册/导出 `RichText` 与 `TextSpan`；`RichText` schema 允许 children 并声明 `link` 事件，`TextSpan` schema 不允许 children。
- 新增 `src/bin/snapshot_rich_text_app.rs` 与 `tests/pty_rich_text.rs`，覆盖结构化富文本 PTY 渲染与链接点击 callback；补充 unit/schema 测试覆盖样式、颜色、合并/清理、callback payload 与非法颜色校验。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。

### [DONE] NR7 — 审阅 NT7
- 确认渲染管线复用（未重写宽字符/截断逻辑）。
- 确认相邻片段合并、空 span 清理、链接命中正确。
- 确认 `Text`/`StyledLabel`/`MarkdownViewer` 等现有文本组件不回归。
- 运行快照 + PTY。
**完成记录（2026-06-07）**：
- 审阅 `src/text/styled_text.rs` 与 `src/widgets/rich_text.rs`，确认 `RichText`/`TextSpan` 通过结构化 `StyledTextSegment` 复用 `spans_from_segments`、`slice_spans_from_segments` 和 `hit_test_link` 管线，未重写宽字符、截断或链接命中逻辑。
- 确认 `normalize_segments` 统一处理相邻同 style/href 合并与空 span 清理；`RichText` 仅接受 `TextSpan` 子节点并通过 `link` 事件发送 URL payload，`TextSpan` schema 不允许 children。
- 确认 `Text` 未受本次管线影响，`StyledLabel` 仍走既有 inline parse + shared segment 渲染路径，`MarkdownViewer` 测试在完整 workspace 验证中通过。
- 验证通过：`cargo test -p atto-ui rich_text`；`cargo test --test pty_rich_text`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。未找到 `run_fixtures.py`，无单独 fixture 套件可运行。

---

## 阶段三：M3 reconciler MVP + M4 主循环

### [DONE] NT8 — react-reconciler HostConfig 骨架 + 节点 id + 静态渲染（U.1）
**文件**：新增 `packages/react/`（`src/reconciler.ts`、`src/host.ts`）
**现状**：尚无 React 库；除 `SetTree` 外所有 `TreeOp` 靠 `ComponentSpec.id` 定位，React host 节点无天然 id（§10.4）。
**步骤**：
1. 引入 `react`/`react-reconciler`，搭 mutation 模式 HostConfig 骨架（LegacyRoot）。
2. host instance：`{ id, type, props, children, windowId }`；`createInstance` 时自增计数器→string，写入将提交的 `ComponentSpec.id`。
3. 子树首次挂载→构造 `ComponentSpec` 树→提交（`SetTree` 或批量插入）。
4. `finalizeInitialChildren`→false；`getPublicInstance`→instance；调度类方法代理 `setTimeout`/`clearTimeout`。
**测试**：reconciler 单测——渲染 `<vstack><label/></vstack>` 断言产出的 spec/ops；headless 渲染出文本。
**验收**：静态 React 树能渲染到一个窗口。
**完成记录（2026-06-07）**：
- 新增 `packages/react/` TypeScript 包，依赖 `react`/`react-reconciler` 与 `@atto-ui/core`，包含 package lock、typecheck/build/test 脚本，并忽略本地 `node_modules` 与构建产物。
- `src/host.ts` 实现 host container 与 host instance 模型：实例在 `createInstance`/`createTextInstance` 时分配稳定 string id，保存 `{ id, type, props, children, windowId }`，将 lower-case host type 映射为 runtime component type，并过滤 React 内部 props / 函数事件 props。
- `src/reconciler.ts` 接入 `react-reconciler` LegacyRoot mutation HostConfig：补齐必需生命周期、调度代理 `setTimeout`/`clearTimeout`/`queueMicrotask`，首次静态提交时将单根 React 子树转换为 `ComponentSpec` 并通过 `set_tree` flush 到目标窗口。
- 新增 JS 测试：纯 reconciler 测试断言 `<vstack><label/></vstack>` 产出的 `set_tree` spec/op；headless 测试经真实 `AppHost` 渲染 React 树并在 snapshot 中断言文本。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NR8 — 审阅 NT8
- 确认节点 id 稳定、唯一、随实例生命周期管理。
- 确认 HostConfig 必需方法齐备、mutation 模式配置正确。
- 运行 reconciler 单测 + headless。
**完成记录（2026-06-07）**：
- 审阅 `packages/react/src/host.ts` 与 `packages/react/src/reconciler.ts`：确认 mutation HostConfig 使用 `LegacyRoot`，声明 `supportsMutation=true` / persistence、hydration 关闭，具备静态提交所需的 create/append/insert/remove/container/text/commit 调度方法，`resetAfterCommit` 统一 flush 当前静态树。
- 确认 host instance 在 `createInstance` / `createTextInstance` 时分配稳定 string id；默认 container id 前缀使用进程级递增值避免不同 root 默认冲突，实例挂载时同步 `parent` 与 `windowId` 到子树。
- 补充 `packages/react/__test__/reconciler.cjs` 覆盖 parent/windowId 生命周期、同一 React 实例重渲染时 id 保持稳定，以及默认 root id 前缀唯一性；现有 headless 测试继续验证真实 `AppHost` 静态渲染文本。
- 验证通过：`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NT9 — props/子节点增删/事件 op 映射（U.1）
**文件**：`packages/react/src/host.ts`、`src/reconciler.ts`
**现状**：NT8 已能静态渲染；需支持更新与增量。依赖 NT6 的 `InsertBefore`（或回退 index 版）。
**步骤**：
1. `prepareUpdate`/`commitUpdate`：props diff→批 `SetProp`；事件 prop 增删→`BindEvent`/`ClearEvent`。
2. 子节点：`appendChild`/`insertBefore`→`InsertBefore`（已挂载节点→Rust 侧等价 Move）；`removeChild`→`Remove`；`clearContainer`→批量 `Remove`/`SetTree(空)`。
3. children 顺序镜像（若用 index 版需据此换算）。
4. op 累积：commit 期间 push 进 buffer，`resetAfterCommit` flush（分桶在 NT13 完善）。
**测试**：reconciler 单测——`useState` 改 text→`SetProp`；列表增删/重排→正确 op 序列；事件 bind/clear 时机正确。
**验收**：动态更新、增删、重排、事件绑定均产出正确 `TreeOp`。
**完成记录（2026-06-07）**：
- `packages/react/src/host.ts` 增加 pending `TreeOp` 缓冲与事件 binding 状态；初始挂载/根替换继续走 `set_tree`，提交期 props/text diff 走精确 `set_prop`，子节点新增与已挂载重排走 `insert_before`，删除走 `remove`。
- `prepareUpdate`/`commitUpdate` 现在计算 host props 与事件 prop diff：事件新增分配 native callback handle 并发 `bind_event`，事件移除发 `clear_event`，handler 函数替换只更新实例记录，不重复 re-bind。
- 扩展 `packages/react/__test__/reconciler.cjs` 覆盖 `useState` 文本更新、列表新增/重排/删除、事件 bind/handler 更新/clear；headless React 测试继续通过真实 `AppHost` 路径。
- 验证通过：`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test --prefix packages/core`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NR9 — 审阅 NT9
- 确认重排的 move 判定正确（已挂载→Move，新建→Insert）。
- 确认事件 prop 仅在增删时 Bind/Clear（不每次 render 重绑）。
- 确认 diff payload 精确（无多余 SetProp）。
- 运行 reconciler 单测。
**完成记录（2026-06-07）**：
- 审阅 `packages/react/src/host.ts` 与 `reconciler.ts`：确认已挂载子节点重排统一发 `insert_before`，由 Rust `InsertBefore` 对已存在 child id 执行 move 语义；补充 append-to-tail 重排回归测试覆盖 `anchor_id: null` 的已挂载 move。
- 审阅事件 prop diff：新增事件才 `bind_event`，移除事件才 `clear_event`，handler 函数替换只更新 instance binding，不重复 re-bind；现有回归测试继续覆盖 handler 更新零 op。
- 发现并修复 props diff 缺口：React prop 删除此前不会产出 op 且会保留旧 props；新增 runtime `TreeOp::ClearProp`、增量 tree 应用、Node/Python 转换、`@atto-ui/core` 类型与 React `clear_prop` 映射，确保删除 prop 不产生多余 `set_prop` 且 runtime 恢复组件默认值。
- 验证通过：`cargo fmt`；`cargo clippy --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NT10 — `render()` + tick 主循环（U.2）
**文件**：`packages/react/src/render.ts`
**现状**：`AppHost::step()` 在 `tickRate=0` 下非阻塞（§5.2）。
**步骤**：
1. `render(element, { cols?, rows?, singleWindow? })`：建 `AppHost`→建 container→`createContainer(LegacyRoot)`→`updateContainer`。
2. tick 微循环（`setImmediate`）：`step()`→（NT11 分发）→React flush→op flush；返回 `RenderHandle{stop,host}`。
3. 退出（Ctrl+Q→`step` 返回 false）→cleanup 恢复终端。
**测试**：PTY——启动渲染、退出干净恢复终端。
**验收**：React 树能持续渲染并响应 tick；进程退出不留终端残状态。
**完成记录（2026-06-07）**：
- 新增 `packages/react/src/render.ts`，导出 `render(element, { cols?, rows?, singleWindow?, headless?, idPrefix? })` 与 `RenderHandle{ host, root, windowId, stop }`；`render()` 构造 `AppHost(tickRate=0)`、创建单窗口 runtime root、挂载 React LegacyRoot，并用 `setImmediate` tick loop 调 `host.step()`，确保 Promise/IO 可继续运行。
- 为真实终端退出补齐显式 cleanup：`src/app/run.rs` 新增 `AppHost::restore_terminal()`，Node binding 暴露 `AppHost.dispose()`；`RenderHandle.stop()` 和 Ctrl+Q/`step()==false` 路径会卸载 React root、关闭窗口并恢复 raw mode/alternate screen/cursor/mouse 状态。
- 更新 `@atto-ui/core`/native 类型声明和 `@atto-ui/react` 导出；新增 React headless 测试覆盖持续 tick 与事件循环让出，新增 Python stdlib PTY 驱动的 JS 测试覆盖启动绘制、Ctrl+Q 退出以及 alternate-screen/cursor 恢复序列。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`npm run typecheck --prefix packages/react`；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NR10 — 审阅 NT10
- 确认微循环让出事件循环（不阻塞 Promise/IO）。
- 确认退出路径恢复终端（raw mode/光标/鼠标）。
- 运行 PTY。
**完成记录（2026-06-07）**：
- 审阅 `packages/react/src/render.ts`、`crates/atto-ui-node/src/lib.rs` 与 `src/app/run.rs`，确认 `render()` 以 `tickRate=0` 构造 `AppHost`，用 `setImmediate` 驱动非阻塞 `host.step()`，`RenderHandle.stop()` 与 `step()==false` 路径会卸载 React root、关闭窗口并调用 `AppHost.dispose()` 恢复终端。
- 补充 `packages/react/__test__/render.cjs`：通过 `setTimeout` 触发 React state 更新并等待 snapshot 变化，覆盖 tick 微循环让出事件循环且不会阻塞 timer/Promise 驱动的 React 更新。
- 补充 `packages/react/__test__/render_pty.cjs`：在原有 alternate-screen 与 cursor restore 断言外，增加 mouse-capture restore 序列和 PTY raw mode flags 恢复检查。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NT11 — 事件分发桥（U.3）
**文件**：`packages/react/src/events.ts`
**现状**：UI 事件经 `CallbackRegistry` 收集，`drainCallbacks()` 拉取（§10.8）。
**步骤**：
1. `callbackId → 最新 handler` Map：`callbackId` 绑定一次、Map 始终指向最新闭包；仅事件 prop 增删时 `BindEvent`/`ClearEvent`。
2. tick 内 `drainCallbacks()`→查表→调 handler→`setState`→React flush→op flush。
3. 组件卸载时 `ClearEvent` + 回收 callbackId。
**测试**：reconciler/集成单测——handler 不重复 bind；卸载后不再触发；PTY——点击 Button→`onClick`→`setState`→屏幕更新（计数器 +1）。
**验收**：UI 事件→React 状态→重渲染闭环成立；无 handler 泄漏/stale。
**完成记录（2026-06-07）**：
- 新增 `packages/react/src/events.ts`，维护 `callbackId -> 最新 handler` 分发表；React host container 在事件 prop 初始创建、handler 更新、事件清理和组件卸载时同步注册、更新和移除 handler。
- `render()` tick loop 在 `host.step()` 后执行 `host.drainCallbacks()` 并分发到 React handler，handler 内 `setState` 会经 reconciler commit 刷新 TreeOp，实现 UI 事件到 React 状态再到屏幕更新的闭环。
- 组件事件清理现在会发送 `clear_event`、删除 JS handler 映射，并通过新增 `AppHost.releaseCallback()` 回收 Node binding callback handle；native `drainCallbacks()` 会丢弃已释放 callback 的 stale invocation，避免卸载后触发。
- 补充测试：reconciler 单测覆盖 handler 更新不重复 bind、卸载 clear/release 后 stale callback 不触发；headless/render 集成测试覆盖 callback 分发后状态更新；新增 PTY 点击 Button 测试覆盖真实终端点击后 `onClick -> setState -> screen update`。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`npm run typecheck --prefix packages/react`；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NR11 — 审阅 NT11
- 确认闭环无重入/丢事件；callbackId 回收正确。
- 确认 LLM 流式与 UI 共存（`for await` 灌 `setState` 不阻塞，§5.2）。
- 运行 PTY + 流式示例。
**完成记录（2026-06-07）**：
- 审阅 `packages/react/src/events.ts`、`host.ts`、`render.ts` 与 `crates/atto-ui-node/src/lib.rs`：确认 tick 中 `host.step()` 后统一 `drainCallbacks()`，分发器按 `callbackId -> 最新 handler` 查表执行，handler 内 `setState` 可同步触发 React commit 与 TreeOp flush，不需要重复 bind。
- 确认事件清理/卸载会先发送 `clear_event`、删除 JS handler 映射并调用 `AppHost.releaseCallback()`；Node `drainCallbacks()` 会过滤已释放 callback id，避免 stale invocation 在卸载后触发。
- 补充 `packages/react/__test__/render.cjs` 的 `for await` 流式回归：模拟 LLM chunk 流持续 `setState`，验证 tick loop 不阻塞 Promise/timer，最终 headless snapshot 渲染到完整流式文本；既有 headless 与 PTY 点击 Button 用例继续覆盖 `onClick -> setState -> screen update` 闭环。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`npm run typecheck --prefix packages/react`；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

---

## 阶段四：M5 文本子系统

### [DONE] NT12 — React 文本组件（U.5）
**文件**：`packages/react/src/text.ts`
**现状**：NT7 已提供 `RichText`/`TextSpan`；`MarkdownViewer` 已注册（`crates/atto-ui-markdown`）。
**步骤**：
1. `createTextInstance(text)`→`TextSpan`；`commitTextUpdate`→`TextSpan` 的 `SetProp text`。
2. 内联组件 `<Text>`/`<B>`/`<I>`/`<U>`/`<S>`/`<Link href>`→设置子 `TextSpan` style flags；`<Text>` 作 `RichText` 容器。
3. `<Link href onClick>`→绑 `link` 事件，payload=url 路由到 `onClick`。
4. `<Markdown>{md}</Markdown>`→`MarkdownViewer`（props `markdown`）。
**测试**：快照——`<B>` 粗体、`<Text>hi {name}</Text>` 合并、块级 markdown；PTY——点击链接触发。
**验收**：React 文本/内联样式/markdown 渲染正确，文本片段在 Rust 侧合并。
**完成记录（2026-06-07）**：
- 新增 `packages/react/src/text.ts` 并从 `packages/react/src/index.ts` 导出 `Text`、`B`、`I`、`U`、`S`、`Link`、`Markdown`；`Text` 生成 `RichText` 容器和多个 `TextSpan` 子节点，样式 flags 写入 `TextSpan` props，不在 JS 侧合并相邻片段。
- 保留 HostConfig 原生 text node 路径：`createTextInstance` 继续创建 `TextSpan`，`commitTextUpdate` 继续对该 `TextSpan` 发送 `set_prop text`；新增 `markdownViewer` host 类型映射到 runtime `MarkdownViewer`。
- `Link href onClick` 通过 `RichText` 的 `link` 事件绑定，按 payload URL 路由到对应 `Link.onClick`；`Markdown` 将文本 children 或 `markdown` prop 映射到 `MarkdownViewer.markdown`。
- 补充测试：reconciler 快照覆盖 raw text `TextSpan` 更新、`Text` 内联 bold/italic/underline/strike/link props、MarkdownViewer 映射与 link callback 分发；headless 测试覆盖 MarkdownViewer native 构建；新增 PTY 链接点击测试覆盖真实终端点击后 `Link.onClick -> setState -> screen update`。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`npm run typecheck --prefix packages/react`；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NR12 — 审阅 NT12
- 确认文本节点合并发生在 Rust 侧（`RichText`），JS 仅产 `TextSpan`。
- 确认链接事件 payload 与 onClick 路由正确。
- 确认 `<Markdown>` 与 viewer 属性映射一致。
- 运行快照 + PTY。
**完成记录（2026-06-07）**：
- 审阅 `packages/react/src/text.ts` 与 `packages/react/src/host.ts`：确认原生 text node 和 `<Text>` 内联 children 都只产出 `TextSpan` 子节点，JS 侧不合并相邻片段；Rust `RichText::segments()` 继续通过 `normalize_segments` 清理空 span 并合并相邻同 style 片段。
- 审阅 `Link` 路由与事件分发：`RichText` link 事件 payload 仍为 URL string，React `Text` 的 `onLink` handler 按 payload 路由到对应 `Link.onClick`，并保留 `onLink`/`onLinkClick` 透传；PTY 链接点击覆盖真实事件路径。
- 审阅 `<Markdown>` 映射：children 或 `markdown` prop 统一映射到 `MarkdownViewer.markdown`，camelCase options 转为 runtime snake_case props，`onLink` 映射到 MarkdownViewer `link` 事件。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm run typecheck --prefix packages/react`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。

---

## 阶段五：M6 Window 映射

### [DONE] NT13 — 虚拟 DesktopContainer + `<Window>` host 节点 + op 分桶（U.4）
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
**完成记录（2026-06-07）**：
- `@atto-ui/react` 新增虚拟 desktop root：`createDesktopRoot()` 使用 `DesktopContainer` 模式，root 直接子节点运行时限制为 `Window`/`MenuBar`/`StatusBar`；普通组件直接挂 desktop root 会报错。
- 新增 `packages/react/src/desktop.ts` wrapper：`Window`、`Desktop`、`MenuBar`、`Menu`、`MenuItem`、`StatusBar`；`render()` 默认使用 desktop root 并在 `singleWindow !== false` 时自动包全屏 `Window`，`singleWindow:false` 支持用户声明多个窗口。
- `Window` host 节点作为虚拟节点处理：mount/unmount 映射到 `addDynamicWindow`/`closeWindow`，`title`/`rect` prop 更新映射到 `setTitle`、`moveWindow`、`resizeWindow`；window 根子树变化使用该窗口的 `set_tree`，普通子树增量 op 按 `windowId` 分桶后逐窗口 `applyTreeOps`。
- 为避免 chrome 节点空壳，Node/core 新增 `setMenuBar` 与 `setStatusBar`；React `MenuBar`/`StatusBar` 会设置 native desktop 固定槽位，`MenuItem.onClick` 经既有 callback registry/drainCallbacks 进入 JS。
- 补充测试覆盖两窗口 op 分桶、window 开关和 title/rect 更新、跨窗口 Context、desktop root 非法子节点、MenuBar/StatusBar lowering、`render()` 单窗口自动包装和 `singleWindow:false` 多窗口 headless 路径；Node binding 冒烟覆盖 native chrome setter。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm run typecheck --prefix packages/react`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NR13 — 审阅 NT13
- 确认 DesktopContainer 仅接受合法子节点，非法子节点编译期/运行期被拦。
- 确认 op 分桶无串窗口；windowId 继承正确。
- 确认 `singleWindow` 与多窗口路径一致。
- 运行 PTY + 单测。
**完成记录（2026-06-07）**：
- 审阅 `packages/react/src/host.ts`、`desktop.ts`、`reconciler.ts` 与 `render.ts`：确认 desktop root 直接子节点运行时限制为 `Window`/`MenuBar`/`StatusBar`，`Window` 内禁止虚拟 chrome 节点，`MenuBar`/`Menu`/`MenuItem` 子树也有运行时结构校验；编译期精确 JSX/host 组件子节点约束已明确纳入后续 `NT14` 的 JSX 类型任务。
- 确认窗口映射与 op 路由：`Window` mount/unmount 走 `addDynamicWindow`/`closeWindow`，title/rect 更新走 `setTitle`/`moveWindow`/`resizeWindow`；普通子树继承最近 `Window.windowId`，pending `TreeOp` 按 `windowId` 分桶，窗口根重建会丢弃同窗口旧增量 op，避免串窗口。
- 确认 `singleWindow` 与多窗口路径共用 `createDesktopRoot`：默认 `render()` 自动包全屏 `Window`，`singleWindow:false` 保留用户声明的多窗口树；`RenderHandle.windowIds()` 从 desktop root 当前窗口子节点派生。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

---

## 阶段六：M7 组件库 + TS 类型

### [DONE] NT14 — host 组件库 + JSX 类型 + 受控输入（U.6）
**文件**：`packages/react/src/components.ts`、`src/jsx.d.ts`
**现状**：内置组件已注册（Button/TextBox/ListBox/TableView/VStack/HStack/Grid 等）；需 React wrapper 与类型。
**步骤**：
1. intrinsic elements + `jsx.d.ts`；wrapper：`<Button onClick>`/`<TextBox value onChange>`/`<ListBox>`/`<Table>`/`<VStack>`/`<HStack>`/`<Grid>`；补齐 `Desktop`/`Window`/`MenuBar`/`Menu`/`MenuItem`/`StatusBar` 的 JSX 子节点类型约束。
2. 统一事件 prop 约定（`onClick`/`onChange`/`onSelect`）→ atto-ui 事件名映射。
3. 在 napi 生成的 native `.d.ts` 上扩展组件 props 类型。
4. 受控输入回环：`<TextBox value onChange>`，确认外部 `SetProp value` + 变更事件不打架（§10.7 风险）。
**测试**：`tsc` 通过；PTY——各组件交互；受控 TextBox 输入正确。
**验收**：常用组件有类型化 React 封装；受控输入无回环抖动。
**完成记录（2026-06-07）**：
- 新增 `packages/react/src/components.ts`，导出 `Button`、受控 `TextBox value/onChange`、`ListBox`、`Table`/`TableView`、`VStack`、`HStack`、`Grid` typed wrappers；事件约定统一为 `onClick`、TextBox `onChange(value,event)`、列表/表格 `onSelect`/`onChange(index,event)`。
- 新增 `packages/react/src/jsx.ts` 与 `src/jsx.d.ts`，为 atto-ui host intrinsic elements 提供 JSX 类型，并补齐 desktop/window/menu/status slot 组件的 children 类型声明；`index.d.ts` 会引用 `jsx` 类型模块。
- 补齐 runtime change payload：TextBox/TextArea 发送当前 string，Checkbox 发送 bool，Slider 发送 f64，RadioGroup/ListBox/TableView/TabView 发送 selection u64；同步更新 schema payload 元数据，避免 React 受控 wrapper 读取宿主状态或猜测值。
- 新增 TSX 类型用例、reconciler wrapper lowering/payload 分发测试、headless 受控 TextBox 回归，以及 PTY components 用例，覆盖 TextBox 输入、Button 点击、ListBox 与 Table 选择更新。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`npm run typecheck --prefix packages/react`；`npm run build --prefix packages/react`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test --prefix crates/atto-ui-node`；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NR14 — 审阅 NT14
- 确认事件 prop 约定一致、类型精确。
- 确认受控输入不出现光标跳动/重复字符。
- 运行 `tsc` + PTY。
**完成记录（2026-06-07）**：
- 审阅 `packages/react/src/components.ts`、`jsx.ts`、`desktop.ts` 与 `host.ts`，确认 wrapper 事件约定统一映射到 runtime 事件名：`Button.onClick -> click`、`TextBox.onChange(value,event) -> change`、`ListBox`/`Table` selection change payload 为 number；补正 `MenuItem.onClick` 类型为 `AttoUiEventHandler`，允许读取 callback event。
- 修复受控 `TextBox` 回环缺口：React wrapper 标记受控 text host；callback 分发后若 runtime payload 与当前 React `text` prop 不一致，会立即回写当前受控值，覆盖拒绝/转换输入场景，避免 native buffer 与 React state 脱同步；接受输入时不会产生重复回写 op。
- 修复重复输入风险：Rust `TextBox` 现在与 `TextArea`/scroll/typeahead 一致忽略 `KeyEventKind::Release`，补充 release 不插入文本的单测。
- 收紧 JSX 类型：raw `<grid>` 改用 runtime snake_case `row_gap`/`column_gap` host props，camelCase 仅由 `Grid` wrapper 接受；补充 TSX 类型负例。
- 确认 PTY 覆盖受控 TextBox happy path、Button 点击、ListBox/Table 选择更新；新增 reconciler 回归覆盖受控 TextBox 拒绝输入回写与接受输入无重复 op。React JSX 对具体 wrapper 子元素身份仍受 TypeScript/React JSX element erasure 限制，运行时结构校验继续覆盖 Desktop/Menu 非法子节点。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`npm run typecheck --prefix packages/react`；`cargo test -p atto-ui widgets::textbox::tests::key_release_does_not_insert_text`；`npm test --prefix packages/react`；`cargo test --all --all-targets`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test --prefix crates/atto-ui-node`；`npm exec --yes --package=typescript@5.9.3 -- tsc -p packages/core/tsconfig.json --noEmit`；`npm test --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NT15 — `@atto-ui/core` 命令式构造器（L.2）
**文件**：`packages/core/src/`
**现状**：低层用法需不依赖 react 的 spec 构造器（§6.4）。
**步骤**：提供 `VStack(...)`/`Text(...)`/`Button(...)` 等薄包装 spec 对象的构造器（类型安全），供低层用法与作为 React 库底层。
**测试**：单测构造 spec 与手写 JSON 等价；`tsc` 通过。
**验收**：不使用 React 也能类型安全地构树。
**完成记录（2026-06-07）**：
- 新增 `packages/core/src/builders.ts` / `builders.js`，导出纯 TS/CJS 命令式构造器：`component`、`child`/`withLayout`/`withMeta`/`tab`，以及 `Text`、`Label`、`Button`、`TextBox`、`TextArea`、`Checkbox`、`RadioGroup`、`Slider`、`Spinner`、`ProgressBar`、`ListBox`、`TableView`、`VStack`、`HStack`、`Grid`、`Border`、`Visibility`、`Divider`、`Spacer`、`Splitter`、`TabView`、`TextSpan`、`RichText`、`StyledLabel`、`Disclosure`、`TypeAhead`、`CommandPalette`。
- 构造器返回标准 `ComponentSpec`/`ComponentSpecChild` plain object，自动丢弃 `undefined` props/events，支持 camelCase 便捷入参到 runtime snake_case 属性映射，并将 `onClick`/`onChange`/`onSubmit`/`onLink` 等事件别名映射为底层 `events` callback handle。
- `packages/core/index.ts` 和 `index.js` 统一 re-export 构造器；新增 `packages/core/__test__/builders.cjs` 断言构造器输出与手写 JSON 等价，扩展 `__test__/types.ts` 覆盖构造器返回值非 `any`、callback handle/string 参数类型约束与 child layout/meta 类型。
- 修复当前 TypeScript 版本下 `moduleResolution: "Node"` 的阻塞性弃用错误：`packages/core` 与 `packages/react` 改用 `module`/`moduleResolution: "Node16"`，保持后续 `tsc` 验证可运行。
- 验证通过：`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm run typecheck --prefix packages/core`；`npm test --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`npm test --prefix crates/atto-ui-node`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

### [DONE] NR15 — 审阅 NT15
- 确认构造器产出的 spec 与核心类型一致、无字段遗漏。
- 运行单测 + `tsc`。
**完成记录（2026-06-07）**：
- 审阅 `packages/core/src/builders.ts` / `builders.js`、`NODE_BINDING.md` §6.4、runtime `schemas()` 与 Python 低层 helper，发现文档示例和 Python 已暴露的 `ChatMessageList` 及相关 Markdown/Terminal/FileTree/Chat 构造器未在 `@atto-ui/core` 中补齐。
- 补齐 `MarkdownViewer`、`TerminalEmulator`、`FileTreeNode`、`FileTree`、`ChatTextMessage`、`ChatFileMessage`、`ChatToolCallMessage`、`ChatArtifactMessage`、`ChatMessageList`、`ChatInputMode`、`ChatInputPanel` 构造器；输出保持标准 `ComponentSpec`/`ComponentValueMap` plain object，camelCase 入参映射到 runtime snake_case props，事件别名映射到 string callback handle。
- 扩展 `packages/core/__test__/builders.cjs` 与 `__test__/types.ts`，覆盖新增构造器输出与类型约束，确认构造器 spec 与核心 runtime 字段一致且回调 handle 不泄漏为 `any`。
- 验证通过：`npm run typecheck --prefix packages/core`；`npm test --prefix packages/core`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`npm test --prefix crates/atto-ui-node`；`git diff --check`。未找到 `tools/run_fixtures.py`，无独立 fixture 套件可运行。

---

## 阶段七：M8 测试 + 示例

### [DONE] NT16 — reconciler 单测矩阵（T.1）
**文件**：`packages/react/__test__/`
**步骤**：mount/update/增删/重排/事件 bind-clear → 断言产出的 `TreeOp` 序列（纯 JS，不进 native）。
**测试**：覆盖 §10.4 映射表每一类操作；含 move 判定、事件 clear 时机边界。
**验收**：HostConfig 行为有回归护栏，CI 内运行。
**完成记录（2026-06-07）**：
- 新增 `packages/react/__test__/reconciler_matrix.cjs`，使用纯 JS mock `AppHost` 覆盖 HostConfig 到 `TreeOp` 的矩阵：初始 mount `set_tree`、props `set_prop`/`clear_prop`、事件 `bind_event`/`clear_event`、raw text `commitTextUpdate`、新增子节点 append/anchor insert、已挂载节点重排 move、remove 前 clear_event、`clearContainer` 空树替换、多窗口 op 分桶和窗口关闭不进入 TreeOp。
- 将新增矩阵接入 `packages/react/package.json` 的 `npm test`，确保 CI 运行；矩阵断言精确到 op 顺序、按 window 分桶后的 apply 调用形态、callback 释放与 stale callback 不再分发。
- 验证通过：`npm run build --prefix packages/react && node packages/react/__test__/reconciler_matrix.cjs`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm run typecheck --prefix packages/core`；`npm run typecheck --prefix packages/react`；`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`（`crates/atto-ui-node`）；`npm test --prefix crates/atto-ui-node`；`npm test --prefix packages/core`；`npm test --prefix packages/react`；`git diff --check`。

### [DONE] NR16 — 审阅 NT16
- 确认矩阵覆盖全（无遗漏 op 类型/边界）。
- 确认断言精确（op 顺序、分桶）。
- 运行单测。
**完成记录（2026-06-07）**：
- 审阅 `packages/react/__test__/reconciler_matrix.cjs` 与 HostConfig lowering，确认矩阵覆盖初始 mount `set_tree`、props `set_prop`/`clear_prop`、事件 `bind_event`/`clear_event`、raw text `set_prop text`、新增 append/anchor insert、已挂载节点 move、remove、`clearContainer` 空树替换、多窗口 op 分桶与窗口关闭不进入 TreeOp。
- 补齐矩阵边界：handler 替换复用原 callback 且不产生 TreeOp；非尾部 move 精确使用 `anchor_id`；`clearContainer` 释放 callback handle，stale callback 不再分发。
- 确认断言精确到 `applyTreeOps(windowId, op|ops[])` 调用顺序、同窗口多 op 批量形态、跨窗口分桶顺序、callback release 顺序与 stale dispatch 结果。
- 验证通过：`npm run build --prefix packages/react && node packages/react/__test__/reconciler_matrix.cjs`；`cargo fmt`；`cargo clippy --workspace --all-targets -- -D warnings`；`cargo test --all --all-targets`；`npm run typecheck --prefix packages/react`；`npm test --prefix packages/react`；`git diff --check`。未找到 `tools/run_fixtures.py`，无单独 fixture 套件可运行。

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
