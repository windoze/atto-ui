# Demo: 11-markdown-viewer

演示如何使用 `atto_ui_markdown::MarkdownViewer` 渲染 Markdown 文本。
示例代码已移动到 `crates/atto-ui-markdown/examples/markdown_viewer.rs`。

## 运行

不传参数时，会显示内置示例内容（覆盖 MarkdownViewer 目前支持的所有特性）：

```bash
cargo run -p atto-ui-markdown --example markdown_viewer
```

传入一个 Markdown 文件路径时，会读取文件并渲染其内容：

```bash
cargo run -p atto-ui-markdown --example markdown_viewer -- README.md
```

## 说明

- 窗口标题：打开文件时为文件名；未提供文件时为 `Markdown Viewer`。
- `MarkdownViewer` 会铺满窗口内容区域（可滚动）。
