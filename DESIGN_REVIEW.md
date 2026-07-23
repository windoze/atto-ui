# atto-ui 核心设计审查报告

审查日期: 2026-07-23
审查范围: 核心库 `src/` 全部模块（不含 terminal / editor / chat 相关领域）。
方法: 5 个方向并行深度审查 + 主线交叉核实（git 历史、编译 warning、关键代码行级验证）。

---

## 总体结论

代码卫生**非常好**: 核心 `src/` 零 `unsafe`、`cargo check --lib` 零 warning、几乎没有 `TODO/FIXME/HACK/deprecated` 及中文遗留标记、`src/bin` 16 个测试二进制全部被引用无孤儿。显式的"历史遗迹"已被清理得很干净（例如 commit `a7bd2f4 [T5] Delete cache and Observable dead code`）。

真正的债务集中在**结构层面**，分三类:
1. **功能性 bug**（少数，但真实影响用户）—— 优先修。
2. **复制粘贴导致的实现分叉**（最大的可维护性风险）—— Stack/Grid、值转换、坐标换算、主循环等多处并行副本已开始各自演进。
3. **设计文档与实现不符 / 分层未定型** —— 尤其脚本化控制平面的"只读"承诺与安全边界。

---

## 一、功能性 Bug（优先修复）

### B1. 模态窗口可被后添加的普通窗口视觉覆盖 【严重】
`src/wm/manager/core.rs:73-84`

`add_window` 对任何窗口无条件 `bring_to_front`。当一个 Modal 活动时再添加 Normal/Floating 窗口，新窗口被 push 到 `windows` 末尾（最顶层），`draw.rs` 按 vec 顺序绘制会把它画在 modal 之上。输入仍被 `hit_test` 正确限制在 modal 内 → 用户看到"看得见的窗口点不动、能点的 modal 看不见"的卡死界面。运行期通过 IPC/脚本加窗口极易触发。
**建议**: 存在活动 modal 且新窗非 modal 时，插入到顶部 modal 之下，或 `bring_to_front` 前检查 `active_modal_id()`。

### B2. `restore_focused` 是死逻辑，恢复功能静默失效 【严重】
`src/wm/manager/focus.rs:139-158`

`minimize_focused` 最小化后立即把焦点转移到 `topmost_focusable_id()`（该函数已过滤 Minimized 窗口），而 `focus()`/`focus_next()` 也从不停在最小化窗口上。因此 `focused()` 返回的窗口**永远不是** Minimized，`restore_focused` 里 `state == Minimized` 判断恒假。结果: WM 模式按 `r`、Window 菜单的 "Restore" 项都是无操作（真正可恢复的只有走最小化列表的 `restore_window(id)`）。已行级核实。
**建议**: 让 `restore_focused` 恢复"最近最小化的窗口"，或移除该无效入口（连同 `WindowMenuOp::RestoreFocused` 与 `'r'` 绑定）。

### B3. Grid 缺失指针捕获（capture）路由，与 Stack 行为分叉 【严重】
`src/composable/grid/events.rs` vs `src/composable/stack/events.rs:332-468`

Stack 有完整的 pointer capture（`captured_child` 字段 + down 时 `match res.capture` + drag/up 路由回被捕获子组件 + `translate_to_child`）。Grid **完全没有**这套机制（已核实 `grid/` 内 `captured_child`/`res.capture` 零命中）。Button/Checkbox 在 mouse down 返回 `Capture::Request`，依赖父容器在拖出边界后仍把事件送回它。放进 `VStack`/`HStack` 正常，放进 `Grid` 时 `Capture::Request` 被静默丢弃 → 按下后拖出再松开无法收到 release，按钮状态卡在 pressed。这是 B/S2 复制粘贴分叉产生的真实交互 bug。
**建议**: 把 capture 状态与路由抽成 Stack/Grid 共享逻辑（同时解决 S1）。

### B4. 动态树双真值 + 无回写，交互状态会在回退式 rebuild 时丢失 【严重】
`src/runtime/tree.rs:20-25, 76-108, 388-402`

`ComponentTree` 同时持有 `root: ComponentSpec`（声明真值）与 `view: Box<dyn Component>`（含活 Binding 的实例，已核实）。两条写路径不对称: tree-ops 增量改 view 且成功后回写 root；而用户交互 / `apply_command` 直接改 view 里的 Binding，**不回写 root**。一旦 `apply_ops_incremental` 任一步失败触发 `rebuild_next_or_restore`（从 root spec 重建 view），用户输入被静默丢弃。且 `get_property` 读 view、`dynamic_root_spec()`（供 introspection）读 root，同一属性会给出不同答案。
**建议**: 明确策略——rebuild 前用 `get_property` 把可读属性 reconcile 回 spec，或规定 rebuild 后由宿主重放状态；至少在类型/文档上标注 `root` 是"最后一次 ops 快照"而非当前真值。

---

## 二、安全 / 契约缺陷（脚本化控制平面）

### C1. `DesktopInspector` 号称"只读门面"，实为可变句柄且"读"有副作用 【严重】
`src/inspect.rs:112-114, 218-591`

`SCRIPTING_LAYERS.md`/`CLAUDE.md` 反复称第 1 层是"纯只读门面"，但结构体持有 `desktop: &'a mut Desktop`（已核实），并公开 `set_property`/`invoke`/`click`/`input_text` 等整套写 API。更隐蔽的是连语义上的"读"也强制 `&mut self` 且有副作用: `tree`/`snapshot`/`export_snapshot`/`query` 都会调 `draw_desktop`（触发真实布局与渲染、清 dirty flag）。类型层面完全没有兑现读写分离承诺。
**建议**: 要么把文档改成诚实描述（"可变控制门面"），要么真正拆出 `&Desktop` 只读读取路径，把写 API 收敛到独立的 `DesktopController`。

### C2. Unix socket 无任何权限加固，本地任意用户可驱动 UI 【严重】
`src/ipc.rs:271-300`（已核实 bind 处无 `set_permissions`/`chmod`/umask）

`bind_unix_listener` 直接 `UnixListener::bind`，socket 默认对同机所有用户可连。而协议能力包含 `invoke`（任意语义动作）、`input_text`、`send_keys`（向终端 pane 注入字节）、`display_popup`（可带 argv 启动命令）——等于给本地任意进程一个远程控制/命令执行面。`remove_stale_socket` 还有 connect-then-bind 的 TOCTOU。
**建议**: 创建后立即 `chmod 0600`（或放 `$XDG_RUNTIME_DIR` 私有目录），文档明确信任边界，`display_popup` 的 argv 执行需显式信任声明。

---

## 三、复制粘贴 / 实现分叉（最大可维护性风险）

### D1. Stack 与 Grid 事件处理近乎逐字复制（~400 行 × 2）
`src/composable/stack/events.rs`(503) 与 `grid/events.rs`(394)。`diff` 证实除类型名、Stack 多出的 capture 块、一行注释外完全相同（`move_focus`/`handle_tab_navigation`/`hit_test_child_scrolled`/`handle_event_impl` 等）。这正是 B3（Grid 缺 capture）与 D-衍生（Grid 无二维方向键导航，`grid/events.rs` 内 `KeyCode::Up/Down` 零命中）能发生的根因: 两份拷贝独立演进。
**建议**: 抽取共享的"focusable children 容器事件核"，Stack/Grid 都委托它。

### D2. 三套主循环并行，定时器/Esc 取消语义不一致
`src/app/run.rs`。新式 `AppHost::step()` 按墙钟推进定时器 + 支持 Esc 取消任务；旧式自由函数 `run_crossterm_desktop`/`_with_actions`/`_with_actions_and_tasks` 每轮固定 `tick_global_timers()` 一次，且 `run_crossterm_desktop` 不做 Esc 取消。旧路径仍被大量 demo 与下游 crate 使用，没退役。
**建议**: 让自由函数内部委托 `AppHost`，统一逻辑，消除三份几乎相同的循环体。

### D3. 两套 `ComponentValue → Rust 类型` 转换系统
`src/runtime/props.rs:69-220`（build 期 `prop_*`）vs `src/component_api.rs:98-408`（运行期 `ComponentValueCodec`）。同一件事实现两遍且规则不完全一致（如 `EdgeInsets` 解析各写一份，codec 版校验 4 元素长度、props 版宽松）。漏改即 build 期与 set 期行为分叉。
**建议**: 让 `prop_*` 复用 `ComponentValueCodec`，EdgeInsets 解析收敛到单一函数。

### D4. `mouse_coords_local_to_area` 有 5 份实现
`geom.rs:71`、`widgets/util.rs:82`（两个 `pub(crate)` 版本已重复）、`slider.rs:333`、`styled_label.rs:148`、`tab_view.rs:356`（三个 widget 私有重抄）。坐标换算是命中测试核心，5 份拷贝任一偏差都会造成难查的点击错位。（radio/textbox/table 已正确复用 util 版本。）
**建议**: 统一到 `geom.rs` 单一 `pub(crate)`，删其余副本。

### D5. inspect.rs 三套并行 tag→属性/动作 dispatch（L1 收敛未完成）
`src/inspect.rs:1220-1606`。`find_by_tag` 统一收敛只做了一半: `component_find`/`component_find_mut` 已成一行 shim（应删除直接调用），但 menu / window / component 三个独立 dispatch 家族（~380 行）仍在，`get_property`/`set_property`/`invoke_by_id` 各手写"menu→window→component"三段短路链。这是 `find_by_tag` 引入前逐子树手搓寻址的残留。
**建议**: 让 Window/Menu 也桥接到统一 `get_property`/`apply_command` 反射面，把三段链收敛成单一 `dispatch(id)->Target` 解析。

### D6. 其它重复
- `Property<T>` 与 `Binding<T>` 字段完全相同（`Arc<RwLock<T>>`+`DirtyFlag`），~90 行方法逐字重复（已核实）。Binding 语义上只是 Property 的引用视图，却是完整复制。建议 Binding 内部持有/薄封装 Property。
- Stack/Grid `scrollbars.rs` 仅差 3 行逐字重复（底层数学已正确共享到 `scroll.rs`，重复的是外壳 draw 循环）。
- 窗口 rect 归一化样板在 `events.rs`/`draw.rs` 重复 6 次；自由函数 `contains` 重复 5 处；`focus`/`focus_next`/`focus_previous` 三处重复 auto-hide 样板；`ComponentContext{..}` 全字段字面量重复 58 次（建议加 `for_child()`/`with_focus()` 辅助）。

---

## 四、历史遗迹（可清理）

| 项 | 位置 | 说明 | 处置 |
|---|---|---|---|
| **空目录 `src/cache/`** | `src/cache/` | 内容已于 `a7bd2f4` 删除，git 未追踪、全库零引用（已核实）。 | 直接 `rmdir` |
| **主题字形重复插入** | `src/theme/mod.rs:815-817` 与 `844-846` | tab-separator/active-left/active-right 三个 key 完全重复插入（HashMap 覆盖同值，无功能影响但误导）。 | 删 844-846 |
| **空占位模块** | `src/composable/grid/layout.rs` | 仅一行注释 `// ... (kept in mod.rs for now)`，helper 内联在 mod.rs。 | 删除或迁入 |
| **零使用组件** | `src/composable/windowed_text.rs`(512 行) | 已导出但全库零引用，未注册为动态组件。 | 确认是否对外 API，否则移除 |
| **空 if 块 + 未实现特性** | `src/wm/manager/draw.rs:73-75` | "dim modal 背后窗口"只有注释无实现，且现有 `desktop_dim` 在绘制所有窗口前填充故实际盖不住。 | 实现或删除 |
| **陈旧里程碑注释** | `src/protocol.rs:4-6` | 提到 "M4 server code"，与当前 L1-L4 命名不符。 | 更新注释 |
| **`z_order.rs` 过度拆分** | `src/wm/manager/z_order.rs` | 全文仅一个 3 行 `bring_to_front`（已核实）。 | 并入 focus.rs/core.rs |
| **死变体/死方法** | `spec.rs:146 ValueType::Unknown`、`dirty.rs:56 check_and_clear`、`builtins.rs:635 register_stack match 双分支相同` | 生产未用 / 无意义 match。 | 移除或接入 |
| **通配再导出** | `runtime/mod.rs:19 pub use spec::*` | 把 spec 全部符号（含内部性类型）泄漏到公共 API 面。 | 改显式列举 |

---

## 五、设计观察（非缺陷，供参考）

- **两套 Tab 容器**: `composable/tab_window.rs`(671) 与 `widgets/tab_view.rs`(878) 功能高度重叠，命名近似易混（窗口级 vs 组件级）。建议文档明确定位边界或规划合并。
- **keymap/which-key 引擎位置存疑**: `app/keymap.rs`(762 行完整 trie/超时/歧义状态机) 核心库内部无调用者，唯一驱动方是下游 `atto-editor-app`。建议下沉为独立 crate 或上移到 editor-app。
- **两个语言绑定回调语义分叉** (M1): Node 侧有 `release_callback` + 存活性过滤，Python 侧没有 → Python 下已释放的 handle 仍可能被投递。`CallbackRegistry` 实为"自增 ID 分配器 + 队列"，名不副实（建议更名 `CallbackBus`），存活性过滤应下沉到它。
- **`Property/Binding::update` 无条件标脏** (M5): 即使值未变也 `mark_dirty` 推进 version，触发下游重绘。widget 普遍用 `update`。建议性能敏感处引导 `update_if` 或加 `T: PartialEq` 比较版本。
- **快照裁剪静默丢信息** (inspect M1): `is_bounded_snapshot_value` 对 StringList/Table/List/Map 一律 false、String>1024 静默截断，导致 ListBox items/TableView 行不出现在快照 `properties`，但 `query` 又能取回 → 快照消费者（Python/Node）看到"删过的"状态。建议留占位/长度提示。
- **`ComponentValue::F64` + serde_json**: NaN/Inf 无法序列化，Slider 走 F64 且快照无条件放行 → 非有限浮点会让 IPC 响应序列化报晦涩错误（进程内路径却正常）。建议 codec 层归一化非有限 F64。
- **`WaitCondition` 只有 PropertyEquals，把 NotFound 当"未满足"**: 等一个拼错 tag 的条件不会快速失败，会一直轮询到 timeout，把配置错误伪装成超时。
- **inspect.rs 巨型文件（2165 行）**: 塞了 node/snapshot 类型、Inspector、change tracker、两套 wait 逻辑、两套树构建器（`build_desktop_tree` 与 `build_desktop_snapshot_tree` 近乎重复）。建议按 `inspect/{tree,snapshot,dispatch,wait}.rs` 拆分。
- **协议缺版本位**: envelope 无 `jsonrpc`/`protocol_version` 字段，无能力协商；id 解析失败统一回落 `"invalid"`，并发坏请求无法区分。
- **`min_size_view.rs` overflow 每帧分配离屏 Terminal** (`:206-267`): 绕过 ratatui diff 且反复堆分配，与"高性能渲染"卖点相悖（仅 overflow 激活时）。建议缓存 buffer。

---

## 优先级建议

1. **立即修** (真实 bug): B1 模态覆盖、B2 restore 失效、B3 Grid capture、B4 动态树状态丢失。
2. **安全** (若 socket 面向多用户环境): C2 socket 权限；C1 文档/类型对齐"只读"承诺。
3. **根治分叉** (一次投入长期收益): D1 抽取容器事件核（同时解决 B3 及二维导航）、D3/D4 收敛值转换与坐标换算。
4. **顺手清理**: 第四节历史遗迹清单（多为几行改动，低风险）。
