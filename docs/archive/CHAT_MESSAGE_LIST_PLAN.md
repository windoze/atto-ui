# Chat Message List 组件实现计划

## 目标
在独立 crate 中实现类似 Codex/Claude Code 的聊天消息列表组件，支持多消息类型、滚动与加载、输入区域与可扩展的等待输入模式，并提供可编程访问/更新消息的 API。

## 分步计划（执行状态）
1. **创建新 crate + 最小可用消息列表** ✅
   - 新建 `crates/atto-ui-chat`，加入 workspace。
   - 定义消息数据模型（消息 ID、发送方、状态、内容类型、时间戳）。
   - 提供可编程消息存储/更新 API（基于 `Binding`/`Property`）。
   - 实现最小可用 `ChatMessageList`：可滚动、使用 `MarkdownViewer` 渲染文本消息、支持文件消息的基础展示。

2. **输入区域与“等待输入”模式接口** ✅
   - 新增输入区组件（默认 TextBox）与基础事件回调。
   - 设计可扩展的“等待输入”接口（Yes/No/自定义输入、多选等），提供 API。
   - 组合出完整面板（消息列表 + 输入区）。

3. **消息样式与状态增强** ✅
   - 发送方分离（对齐）与时间分隔线。
   - In-progress 动画（Spinner）与渐进式渲染接口。
   - 提供基础样式配置入口（wrap width / in-progress suffix）。

4. **滚动加载与测试/示例** ✅
   - 支持滚动到顶部时的“加载更多”回调。
   - 增加聊天 Demo（含 mock AI 随机延迟与流式渲染）。
   - PTY 测试用例：后续如需可补充。
