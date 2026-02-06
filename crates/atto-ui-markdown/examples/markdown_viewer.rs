use std::env;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use ratatui::layout::Rect;

use atto_ui::app::{
    CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop_simple,
};
use atto_ui::theme::Theme;
use atto_ui_markdown::MarkdownViewer;
use atto_ui::wm::{Window, WindowKind};

const DEFAULT_MARKDOWN: &str = r#"
# Markdown Viewer

这是 `MarkdownViewer` 的演示。默认示例尽量覆盖当前控件支持的所有 Markdown 特性：

- **粗体**、*斜体*、~~删除线~~
- `行内代码` 与 [链接](https://example.com)
- 无序/有序列表（含嵌套）
- 引用块（含嵌套内容）
- 分隔线
- 代码块（块内可滚动）
- 表格（表内可滚动）

---

## 段落与换行

普通换行会被折叠为一个空格（SoftBreak）。
在行末添加两个空格会产生硬换行（HardBreak）。  
这一行应该从上一行后换到下一行。

## 列表

- 无序列表项 1
- 无序列表项 2（下面是嵌套有序列表）
  1. 嵌套有序 1
  2. 嵌套有序 2

1. 有序列表 1
2. 有序列表 2（包含 `inline code`）

## 引用

> 这是一个引用块，里面也可以有 **强调**、`代码` 和 [链接](https://example.com/docs)。
>
> - 引用中的列表项 A
> - 引用中的列表项 B

## 代码块

```rust
fn main() {
    let long_line = "0123456789 0123456789 0123456789 0123456789 0123456789 0123456789";
    println!("{long_line}");
}
```

## 表格

| Feature | Example | Notes |
| --- | --- | --- |
| Inline styles | **bold**, *italic*, ~~strike~~, `code` | 支持基础行内样式 |
| Link | [example.com](https://example.com) | 链接可点击（回调可选） |
| Long cell | This cell is intentionally very long to exceed the width and require horizontal scrolling. | 表格支持横向滚动 |
"#;

fn usage(program: &str) -> String {
    format!(
        "用法:\n  {program} [markdown_file]\n\n示例:\n  {program}\n  {program} README.md\n\n提示: 使用 cargo 运行时，参数需要放在 `--` 之后:\n  cargo run -p atto-ui-markdown --example markdown_viewer -- README.md\n"
    )
}

fn load_markdown_from_args() -> Result<(String, String)> {
    let mut args = env::args();
    let program = args
        .next()
        .unwrap_or_else(|| "markdown_viewer".to_string());
    let first = args.next();

    if matches!(first.as_deref(), Some("-h" | "--help")) {
        print!("{}", usage(&program));
        std::process::exit(0);
    }

    let Some(path_arg) = first else {
        return Ok(("Markdown Viewer".to_string(), DEFAULT_MARKDOWN.to_string()));
    };

    let title = Path::new(&path_arg)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&path_arg)
        .to_string();

    let bytes =
        std::fs::read(&path_arg).with_context(|| format!("读取 Markdown 文件失败: {path_arg}"))?;
    let markdown = String::from_utf8_lossy(&bytes).into_owned();

    Ok((title, markdown))
}

fn main() -> Result<()> {
    let (title, markdown) = load_markdown_from_args()?;

    let config = CrosstermAppConfig::default()
        .tick_rate(Duration::from_millis(16))
        .mouse_capture(true)
        .cursor(CursorMode::Hide);

    run_crossterm_desktop_simple(config, move |screen: Rect| {
        let theme = Theme::dark();
        let menu = MenuBar::new(vec![]);
        let mut desktop = Desktop::new(theme, menu);

        // 让窗口占满工作区，这样 MarkdownViewer 也能直观看到“铺满窗口”的效果。
        let work = Desktop::layout(screen).work_area;

        let viewer = MarkdownViewer::new(markdown);
        let window = Window::new(WindowKind::Normal, title, work, Box::new(viewer));
        desktop.add_window(window, screen);

        Ok(desktop)
    })
}
