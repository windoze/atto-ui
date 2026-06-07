# 当前执行计划

## 约束说明

- 本文件记录可审阅的执行计划、关键决策和进度更新；不会记录不可审计的内部推理细节。
- `TODO.md` 是任务顺序和完成状态的唯一权威来源。
- 本轮只完成 `TODO.md` 中第一个标题未带 `[DONE]` 的任务，然后提交并停止。

## 初始计划

1. 读取 `TODO.md`，按文件顺序定位第一个标题未带 `[DONE]` 的任务。
2. 检查该任务的要求、依赖、验证要求和完成记录；只在和当前任务直接相关时查看最近提交或相关文件。
3. 如任务无法按原规格完成，且存在具体缺失功能或阻塞问题，则在 `TODO.md` 中添加最小必要的前置任务，提交后停止。
4. 如任务可执行，则最小范围修改代码、测试或文档，避免无关重构和规避性实现。
5. 按要求先运行 `cargo fmt`，再运行 `cargo clippy --all-targets -- -D warnings`，最后运行相关或完整测试；若仅文档变更且已有可复用绿色结果，则记录跳过原因。
6. 更新 `TODO.md`：为完成的任务标题添加 `[DONE]`，并填写本轮完成记录和验证结果。
7. 如阶段级计划没有变化，不更新 `PLAN.md`。
8. 提交本轮所有相关变更，提交信息包含任务编号和简明说明。
9. 提交后停止，不继续处理下一个任务。

## 当前进度

- 已创建初始执行计划。
- 已读取 `TODO.md` 与 `TODO-1.md`，确认首个未完成任务为 `NR4 — 审阅 NT4`。
- 最近提交为 `[NT4] Expose Node AppHost API`，直接对应本次审阅对象；本轮将把该提交内容作为审阅范围。
- 已对照 `PLAN-1.md`、`NODE_BINDING.md`、Node/Python binding 初步审阅，发现 `Rect` 输入只支持对象、不支持规格要求的 `[x,y,width,height]` 数组；同时 `NODE_BINDING.md` 的 §6.1/§6.2 仍保留旧 numeric id 表述。

## NR4 执行计划

1. 阅读 `PLAN-1.md` / `NODE_BINDING.md` 中与 B.1、§6.1 相关的 AppHost API 规格。
2. 对照 Python binding 的 `PyAppHost` 能力，审阅 `crates/atto-ui-node/src/lib.rs` 及相关转换、事件和测试文件。
3. 重点验证方法集、签名、string handle、`tickRate=0` 非阻塞、`drainCallbacks` 载荷完整性，以及 headless 路径是否不依赖真实 PTY。
4. 如发现实现或测试缺口，直接修复并补充测试；如发现无法在本任务内正确修复的前置缺口，则更新 `TODO.md` / `TODO-1.md` 后提交并停止。
5. 运行 `cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、相关 Rust/JS 测试，并视变更范围运行完整测试。
6. 审阅完成后将 `NR4` 在 `TODO.md` 和 `TODO-1.md` 中标记完成，填写完成记录。
7. 检查工作区状态和 diff，提交本轮全部相关变更。

## NR4 修正计划更新

1. 将 Node `addDynamicWindow` 的 `rect` 入参改为 JSON 值并解码对象或 4 元数组，两种形态都转换为 `ratatui::layout::Rect`。
2. 增加 Rust 单测覆盖对象/数组矩形解码和非法矩形错误；更新 JS 冒烟测试覆盖数组输入。
3. 更新 `NODE_BINDING.md` 中 AppHost 和类型映射的 id handle 表述，避免与已完成的 NT3/NT4 行为冲突。
4. 重新生成/更新 napi 类型声明，确保 TS 视角仍表达 `Rect | [number, number, number, number]`。

## NR4 完成进度

- 已完成 Node `Rect` 输入修复、JS 冒烟测试更新、`NODE_BINDING.md` 签名修正和 `index.d.ts` 重新生成。
- 验证已通过：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p atto-ui-node`、`cargo test --all --all-targets`、`npm exec --yes --package=@napi-rs/cli@3.1.5 -- napi build --platform`、`npm test`。
- 已将 `TODO.md` 索引和 `TODO-1.md` 的 `NR4` 标记为完成，并补充完成记录。
- 下一步：检查最终 diff，提交本轮相关变更后停止。
