# Demo: 10-file-dialog

演示如何使用 `atto_ui::dialogs::FileDialog` 打开/保存文件。

## 运行

```bash
cargo run --bin demo-10-file-dialog
```

## 操作

- 在主窗口点击：
  - `Open File...`：打开「选择文件」对话框
  - `Save File...`：打开「保存文件」对话框
- FileDialog 内部：
  - `Tab` / `Shift+Tab`：切换焦点
  - `↑` / `↓`：选择文件/目录
  - `Enter`：
    - 在列表上：进入目录 / 选择文件
    - 在 File name 输入框上：提交（Open/Save）
  - `Backspace`：返回上级目录（当列表聚焦时）
  - `Esc`：取消并关闭对话框

