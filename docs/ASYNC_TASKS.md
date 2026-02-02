# 后台异步任务指南

## 概述

本文档说明如何在 Chatty 应用中使用后台线程更新 UI，例如：
- 文件下载进度条
- 网络请求加载状态
- 长时间运行的计算任务

## 核心原理

使用 `std::sync::mpsc::channel` 实现后台任务与主事件循环的通信：

```
┌──────────────────┐         ┌──────────────────┐
│  后台线程        │         │  主事件循环      │
│                  │         │                  │
│  sender.send()   │────────>│ receiver.try_recv│
│                  │ channel │                  │
└──────────────────┘         └──────────────────┘
```

**优势**：
- ✅ **零竞态条件** - 标准库保证线程安全
- ✅ **即时唤醒** - 有新消息时立即返回
- ✅ **简单直观** - 无需手动管理标记位
- ✅ **多生产者** - 多个后台线程共享同一个 sender

## 快速开始

### 1. 定义应用动作

```rust
#[derive(Clone, Debug)]
enum AppAction {
    UpdateProgress(f64),      // 更新进度
    ShowMessage(String),       // 显示消息
    DataLoaded(Vec<String>),  // 数据加载完成
}
```

### 2. 创建 Channel

```rust
use std::sync::mpsc;

// 在 main 函数中创建
let (action_sender, action_receiver) = mpsc::channel::<AppAction>();
```

### 3. 主事件循环集成

```rust
loop {
    // 1. 渲染 UI
    terminal.draw(|f| desktop.draw(f))?;

    // 2. 非阻塞地检查应用动作
    while let Ok(action) = action_receiver.try_recv() {
        match action {
            AppAction::UpdateProgress(p) => {
                progress.set(p);  // 更新反应式属性
            }
            AppAction::ShowMessage(msg) => {
                status.set(msg);
            }
            // ...
        }
    }

    // 3. 轮询终端事件（带超时）
    if event::poll(Duration::from_millis(50))? {
        let ev = event::read()?;
        desktop.handle_event(&ev, screen);

        // 处理用户输入...
    }
}
```

### 4. 启动后台任务

```rust
// Clone sender 给后台线程
let sender = action_sender.clone();

std::thread::spawn(move || {
    // 执行长时间任务
    for i in 0..=100 {
        std::thread::sleep(Duration::from_millis(50));

        // 发送进度更新
        sender.send(AppAction::UpdateProgress(i as f64 / 100.0)).ok();
    }

    // 发送完成消息
    sender.send(AppAction::ShowMessage("Done!".to_string())).ok();
});
```

## 完整示例

运行示例程序查看完整的下载进度条演示：

```bash
cargo run --example async_progress
```

**功能**：
- 按 `s` 启动模拟下载任务
- 实时更新进度条（每 30ms）
- 下载完成后显示消息
- 按 `q` 退出

## 高级用法

### 多个后台任务

```rust
let sender1 = action_sender.clone();
let sender2 = action_sender.clone();

// 任务 1：下载文件
std::thread::spawn(move || {
    download_file(&sender1);
});

// 任务 2：处理数据
std::thread::spawn(move || {
    process_data(&sender2);
});

// 主循环统一处理所有消息
while let Ok(action) = action_receiver.try_recv() {
    // ...
}
```

### 结合 tokio 异步运行时

```rust
// Cargo.toml: tokio = { version = "1", features = ["rt", "macros"] }

// 创建 tokio 运行时（在独立线程中）
let sender = action_sender.clone();
std::thread::spawn(move || {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let data = fetch_async_data().await;
        sender.send(AppAction::DataLoaded(data)).ok();
    });
});
```

### 错误处理

```rust
#[derive(Clone, Debug)]
enum AppAction {
    Success(String),
    Error(String),
}

// 后台任务
std::thread::spawn(move || {
    match risky_operation() {
        Ok(data) => sender.send(AppAction::Success(data)).ok(),
        Err(e) => sender.send(AppAction::Error(e.to_string())).ok(),
    };
});
```

### 取消任务

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

let cancelled = Arc::new(AtomicBool::new(false));
let cancel_flag = cancelled.clone();

// 后台任务
std::thread::spawn(move || {
    for i in 0..1000 {
        // 检查取消标记
        if cancel_flag.load(Ordering::Relaxed) {
            sender.send(AppAction::Cancelled).ok();
            return;
        }

        // 执行工作...
        std::thread::sleep(Duration::from_millis(10));
        sender.send(AppAction::Progress(i)).ok();
    }
});

// 主循环中取消任务
if user_pressed_cancel {
    cancelled.store(true, Ordering::Relaxed);
}
```

## 性能考虑

### 延迟分析

| 场景 | 延迟 |
|------|------|
| 后台任务发送消息 | < 1μs |
| 主循环接收消息 | < 1ms |
| 最坏情况延迟 | 50ms（poll 超时）|

### 优化建议

1. **批量处理**：一次循环处理多个消息
   ```rust
   while let Ok(action) = receiver.try_recv() {
       handle(action);  // 批量处理，避免重复渲染
   }
   ```

2. **动态超时**：根据是否有待处理消息调整超时
   ```rust
   let timeout = if has_pending_work {
       Duration::from_millis(1)   // 快速轮询
   } else {
       Duration::from_millis(50)  // 正常超时
   };
   ```

3. **防抖动**：避免过度频繁的更新
   ```rust
   let mut last_update = Instant::now();

   while let Ok(action) = receiver.try_recv() {
       if last_update.elapsed() > Duration::from_millis(16) {
           progress.set(new_value);
           last_update = Instant::now();
       }
   }
   ```

## 常见问题

### Q: 为什么用 mpsc 而不是 AtomicBool？

A: `mpsc::channel` 天然支持消息传递，无需手动管理状态：
- 不需要额外的 AtomicBool 标记
- 不需要担心竞态条件
- 标准库实现，经过充分测试
- 代码更简洁直观

### Q: Receiver 能 clone 吗？

A: 不能。`mpsc::Receiver` 不支持克隆（这是设计决策）。主事件循环应该唯一持有 receiver。

### Q: Sender 可以跨线程吗？

A: 可以。`mpsc::Sender` 实现了 `Send + Sync`，可以安全地在线程间传递和克隆。

### Q: Channel 会阻塞吗？

A: 默认的 `mpsc::channel()` 是无界的，`send()` 永不阻塞。如果需要有界 channel，使用 `mpsc::sync_channel(capacity)`。

### Q: 如何处理 Channel 关闭？

A: 当所有 `Sender` 都被丢弃时，`recv()` 会返回 `Err`：
```rust
match receiver.try_recv() {
    Ok(action) => { /* 处理 */ }
    Err(TryRecvError::Empty) => { /* 无消息 */ }
    Err(TryRecvError::Disconnected) => { /* Channel 已关闭 */ }
}
```

## 相关 API

- [`EventQueue::channel()`](../src/reactive/queue.rs) - 创建 channel
- [`Property<T>`](../src/reactive/property.rs) - 反应式属性
- [`Binding<T>`](../src/reactive/property.rs) - 双向绑定

## 更多示例

- [examples/async_progress.rs](../examples/async_progress.rs) - 下载进度条
- [examples/demo.rs](../examples/demo.rs) - 完整演示应用
- [demos/06-data-binding/](../demos/06-data-binding/) - 数据绑定教程
