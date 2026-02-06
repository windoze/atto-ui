# TabView Demo

该演示展示 `TabView` 的核心能力：

- 页签标题渲染（`|`、`>`、`<` 由主题 glyph 控制）
- 页签点击切换
- 动态新增/删除页签
- 程序化选中与头部位置切换（Top/Bottom）

## 运行

```bash
cargo run --bin demo-12-tab-view
```

## 快捷键

- `Ctrl+T`：新增 Tab
- `Ctrl+D`：删除当前 Tab
- `Ctrl+←/→` 或 `Ctrl+P/N`：切换 Tab
- `Ctrl+1..9`：程序化选中指定 Tab
- `Ctrl+H`：切换 Tab 头部位置（上/下）

鼠标点击标题可切换 Tab，点击内容区可与控件交互。
