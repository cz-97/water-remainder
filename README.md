# water-remainder

使用 [GPUI](https://github.com/zed-industries/zed)（Zed 编辑器的 GPU 渲染 UI 框架，经 [`gpui-kit`](https://github.com/longbridge/gpui-kit) 引入）构建的 Windows 喝水提醒工具。常驻托盘、定时弹窗提醒，并记录每一次喝水打卡，形成日历热力图与时间轴。

---

## 功能

- **定时提醒** — 到点后全屏透明浮层提示「该喝水了」，可选择「喝了」或「跳过」；浮层上「上次喝水 / 下次提醒」两处倒计时按秒实时跳动，并附具体时刻
- **智能顺延** — 提醒时间基于**最近一次喝水记录**计算；休眠唤醒后自动重新计算，不会因为合盖而漏提醒
- **喝水记录** — 每次打卡写入本地 SQLite，主窗口以日历热力图 + 当日时间轴展示
- **间隔可调** — 15 ~ 75 分钟，步进式调节（默认 45 分钟）
- **开机启动** — 写入 `HKCU\...\CurrentVersion\Run` 注册表项
- **单实例运行** — 基于命名互斥体，重复启动直接退出
- **窗口状态记忆** — 主窗口位置、尺寸、最大化状态持久化

## 交互入口

| 操作 | 行为 |
| --- | --- |
| 左键单击托盘图标 | 打开 / 唤起主窗口（记录日历） |
| 右键托盘菜单「立即提醒」 | 立刻弹出一次提醒浮层 |
| 右键托盘菜单「退出」 | 保存窗口状态并结束进程 |
| 主窗口标题栏齿轮图标 | 打开设置窗口 |
| 主窗口标题栏关闭按钮 | **隐藏**主窗口，进程继续托盘常驻（窗口不销毁，下次唤回无需重建） |

---

# 技术栈

| 领域 | 选型 |
| --- | --- |
| 语言 | Rust 2024 edition |
| UI 渲染 | `gpui-kit`（`default-features = false`，只启用 gpui / gpui_platform 两层；组件层与图标资源关闭） |
| 托盘与菜单 | `tray-icon` / `muda` |
| 窗口句柄 | `raw-window-handle` + `windows` crate (Win32 / DWM) |
| 时间处理 | `chrono`（clock 特性） |
| 存储 | `rusqlite`（bundled SQLite）+ `r2d2` / `r2d2_sqlite` 连接池 |
| 跨线程通信 | `std::sync::mpsc`（调度线程）+ `futures_channel`（GPUI 事件循环） |
| 资源嵌入 | `embed-resource` + `.rc` 文件（图标） |

---

## 架构总览

```
┌──────────────────────────────────────────────────────────────┐
│                          main.rs                             │
│  单实例检查 → GPUI App 启动 → 事件汇聚（AppEvent 通道）        │
└───────┬──────────────┬──────────────┬──────────────┬─────────┘
        │              │              │              │
   ┌────▼────┐   ┌─────▼─────┐  ┌─────▼─────┐  ┌─────▼──────┐
   │ tray.rs │   │scheduler.rs│ │ config.rs │  │  data.rs   │
   │ 托盘图标 │   │ 定时调度线程│ │ 设置持久化 │  │ SQLite 存储 │
   │ 菜单项   │   │ 命令/事件  │ │ settings  │  │ 聚合 + 明细 │
   └────┬────┘   └─────┬─────┘  └───────────┘  └─────┬──────┘
        │              │                              │
        │   TrayIconEvent / MenuEvent / SchedulerEvent   │
        └──────────────┴──────────────┬───────────────┘
                                      ▼
                       ┌──────────────────────────────┐
                       │        UI 层（GPUI View）      │
                       │ main_window   记录日历 + 时间轴 │
                       │ reminder_window 全屏提醒浮层   │
                       │ settings_window 设置面板       │
                       │ ui.rs 时间/配色/窗口按钮工具    │
                       └──────────────┬───────────────┘
                                      ▼
                       ┌──────────────────────────────┐
                       │ platform.rs（Win32 / DWM）     │
                       │ 圆角·边框·暗色菜单·自启·单实例 │
                       └──────────────────────────────┘
```

### 分层说明

**1. 入口与事件汇聚层（`main.rs`）**

程序的唯一「装配点」。启动时先做单实例互斥检查，随后 `gpui_kit::init(cx)` 初始化 gpui-kit 已启用的层，再以 `QuitMode::Explicit` 启动 GPUI——关闭窗口不等于结束进程（主窗关闭只是隐藏），托盘常驻，退出走托盘菜单。

三个异构事件源被统一包装成 `AppEvent` 枚举，通过一条 `futures_channel::mpsc` 无界通道送入同一个异步消费任务：

- `TrayIconEvent`（托盘点击）— 来自 `tray-icon` 的全局回调
- `MenuEvent`（菜单项）— 来自 `muda` 的全局回调
- `SchedulerEvent`（调度器信号）— 由独立的 OS 线程 `recv` 后转发

这样做的好处是：UI 的所有变更都串行发生在 GPUI 主线程的同一处 `match`，无需在回调里做跨线程的界面操作。

这里还订阅了一次系统唤醒（`cx.on_system_wake`）：唤醒后向调度器发 `Reschedule(Wake)`，把 deadline 按「距上次喝水已过去多久」重算一次。**这个 `Subscription` 必须 `detach()`**——`Platform::run` 是先调用启动闭包、之后才进入 `GetMessageW` 消息循环（`gpui-pre-windows/src/platform.rs:508`），绑在闭包里的 RAII 守卫会在消息循环开始之前就被 drop 掉并注销回调，回调于是永远不会被调用。同一处的 `std::mem::forget(tray)` 处理的是同一个「闭包提前返回」问题。

**2. 调度层（`scheduler.rs`）**

一个专用的 OS 线程，持有 `std::sync::mpsc` 双向通道：

- 输入 `SchedulerCmd`：`Reschedule(Drink | Wake | ChangeInterval)`、`Trigger`、`Stop`
- 输出 `SchedulerEvent::Remind { remaining }`：交给主线程弹窗

两个方向各自一个枚举。这样 UI 侧的 `match` 只面对 `Remind` 一个变体，不需要用 catch-all 分支去吞掉「只发给调度器的命令」——将来调度器新增事件时，编译器的穷尽性检查会直接报出未处理的变体，而不是被静默忽略。

核心是一个 **1 秒粒度的轮询循环**（`recv_timeout` 兼作 sleep），用 `deadline: SystemTime` 表示下次提醒时刻。超时即触发提醒并把 deadline 顺延；`Trigger` 则立即触发但不重置 deadline（因此浮层上能显示「将于 X 分钟后再次提醒」）。

`get_deadline()` 会把「距上次喝水已过去的时间」从间隔中扣除，所以重启程序或从睡眠唤醒后，提醒节奏仍然连续。

**3. 存储层**

拆成两个互不干扰的持久化通道：

- `config.rs` — 设置与窗口状态，纯文本 `key=value`，路径 `%APPDATA%\water-remainder\settings.txt`。零依赖、可读、损坏时逐行回退默认值。`INTERVALS: &[u64]` 是间隔秒数表，设置窗按索引取值、标签由秒数现算「N 分钟」，UI 与调度共用同一份数据源。
- `data.rs` — 喝水记录，SQLite 单表 `drink_records(id, timestamp)` + `timestamp` 索引。开启 WAL 与 `busy_timeout`，`r2d2` 连接池上限 4。内存在进程内分两层：**天级聚合 `DayCounts`（`BTreeMap<NaiveDate, DayStat{count, last}>`，规模只与「有记录的天数」成正比）** 与 **明细 `DETAILS`（`BTreeMap<NaiveDate, Arc<Vec<u64>>>`，点开某天才查一次）**。`save_time()` 落库成功后只增量更新这两层。提醒文案与调度需要的「上次喝水距今」由 `get_elapsed()` 从聚合层取（`last_time()` = 最后一个分组的 `MAX(timestamp)`），**完全不需要明细**。

**记录缓存的生命周期（`data.rs`）**

`COUNTS`（天级聚合）与 `DETAILS`（明细，按需）是记录的唯一来源，`LAST_TIME` 之类的旁路缓存已移除。进程运行期间与数据库只有三类交互：

| 时机 | 数据库操作 |
| --- | --- |
| 首次 `day_counts()`（进程内第一次要记录，含调度线程 `get_deadline()` → `get_elapsed()` 那次） | 一次 `SELECT date(timestamp,'unixepoch','localtime') AS day, COUNT(*), MAX(timestamp) … GROUP BY day` —— **一天一行，不读明细** |
| 首次点开某天，且那天次数 > 0（`day_detail()`） | 一次区间查询 `WHERE timestamp >= ?1 AND timestamp < ?2`，命中 `idx_timestamp` 做索引查找 |
| 每次点击「喝了」 | 一次 `INSERT` |

三个刻意的取舍：

- **选中日的次数为 0 时一条 SQL 都不发**：`day_detail()` 先查聚合层，为 0 就直接返回空 `Arc`，连缓存条目都不建。
- **某天的明细一天最多查一次**：结果进 `DETAILS`，此后（含窗口重绘）都命中内存。`save_time()` 也只在「那天的明细已经加载过」时才把新时间戳追加进已缓存的那条，**绝不因为写入而顺带加载明细**。
- **两层缓存都只增不减、不做失效**：若程序运行期间由外部直接改写 `data.db`，界面不会感知（重启进程即重新加载）。

于是 GPUI「每次 draw 都重跑 `render()`」不再有代价：日历每格是 `counts.count(day)`（`BTreeMap` 查找），时间轴是选中日明细的 `Arc` 克隆，都不产生 `SELECT`。代价是日期边界要由 chrono 自己算（`ui::day_bounds()` 给出该本地日的 `[00:00, 次日 00:00)`），换来的是「按天取明细」永远是一次索引区间查找 —— 比按 `date()` 函数过滤快近两个数量级，也不必给表加 `day` 列（SQLite 不允许生成列里用 `localtime`，`INSERT` 会直接报非确定性错误）。

**4. UI 层**

三个独立的 GPUI 窗口，各自是实现了 `Render` 的 View：

- `main_window.rs` — 记录总览。日历按「今天向前倒推」逐行生成（每行 7 天，行内首次遇到 `day == 1` 时标注月份），配色由 `ui::calendar_color()` 按当日次数映射到蓝色梯度，形成 GitHub 贡献图式热力图；右侧是选中日期的时间轴，今天额外显示「N 分钟前」相对时间。
  - `MainWindow` 结构体只有 `store` / `scheduler` / `selected_date` 三个字段，**不持有任何记录数据**。渲染时只为每个格子读一次 `counts.count(day)`（返回 `usize`）上色；右侧明细先经 `day_detail(selected)` 取到（懒加载 + 缓存），再按 `Arc` 借用渲染；点某天即改 `selected_date` 并 `cx.notify()` 触发新一轮 `render`。
  - `render()` 只做三件事：组装一次性的 `RenderInput`（`today` / `selected` / 天级聚合 / 选中日明细）、调用三个子视图、拼外壳。时刻与记录因此是**显式传参**，`calendar()` / `timeline()` / `title_actions()` 不会各自去读全局状态。三个子视图的返回类型写成具体的 `Div` 而非 `impl IntoElement`，否则返回值会隐式借用 `&mut Context`，同一个 `render` 里就没法把 `cx` 依次交给多个子视图。
  - 关闭按钮**不销毁窗口**：`on_window_should_close` 采集并落盘窗口状态后调 `platform::hide_main_window()`，返回 `false` 让 gpui 吞掉 `WM_CLOSE`（不再交给 `DefWindowProc` 销毁）。隐藏期间窗口收不到 `WM_PAINT`，也没有任何路径会 `notify` 它——**记录入库只写缓存、不通知主窗**，所以日历的刷新完全依赖 `show_main_window` 里那次置脏：唤回时重绘一帧，重新取聚合与选中日明细（都命中缓存）+ `local_date(now())`。
- `reminder_window.rs` — 覆盖整个主显示器的透明 `WindowKind::PopUp` 浮层。文案不预先生成，而是把 `last_drink` / `next_reminder` 两个时间戳存进 View，渲染时按「当前时刻」现算，因此有几点行为：
  - **两处倒计时按秒跳动**。实体创建时 `cx.spawn` 起一个 1 秒周期的后台定时任务，每轮醒来 `cx.notify()` 触发重绘（`notify` → `invalidate_view` 置窗口 dirty 并唤醒平台 waker → 下一帧重跑 `render`）。窗口关闭后弱引用升级失败，任务自行退出，不会泄漏。
  - 文案形如 `您在 1 天 2 小时 15 分 30 秒前喝过水（昨天 15:04:32），将于 12 分 3 秒后再次提醒您（15:37:11）`。时间一律写到秒，并用「从最大非零单位一路展开到秒」的写法，避免出现「1 小时 59 秒」这种有歧义的省略。
  - 括号内是具体时刻 `HH:MM:SS`；日期用相对词表示——当天省略、`昨天`、`前天`、`N 天前`。下次提醒的时刻始终不写日期。
  - 「喝了」写入记录并 `Reschedule(Drink)`，「跳过」仅关闭窗口。
  - 浮层已经存在时**不静默返回**，而是原地更新 `last_drink` / `next_reminder` 并重绘。浮层有可能是在屏幕没亮、显示器正在重枚举的那个瞬间被创建出来的——用户看不见它，而它除了被点击之外不会被自动关闭，静默返回会让此后每一次提醒都被吞掉，直到重启进程。
- `settings_window.rs` — 间隔步进器（受 `INTERVALS` 边界约束，越界时按钮置灰）+ 开机启动开关。两处修改都走 `config::update_settings(&store, |settings| ...)`：加锁、改值、落盘收在这一个函数里，窗口侧只描述「改什么」，不出现 `lock` + `save_store` 的成对代码。间隔变更随后向调度器发送 `ChangeInterval`。

`ui.rs` 是共享工具模块：时间戳换算（`now` / `local_date` / `format_clock` / `format_clock_secs` / `format_span` / `format_day_label` / `relative_to_now`）、配色表 `palette`、热力图取色 `calendar_color`、以及自绘标题栏的三个部件 —— `titlebar()`（左侧可拖拽标题 + 右侧按钮槽）、`titlebar_button()`（**标题栏图标按钮的唯一样式来源**：46×38、图标居中、悬停换底色，尺寸与字体来自文件顶部的 `TITLEBAR_*` / `ICON_FONT` 常量）与 `window_button()`（在共用样式之上附加 `WindowControlArea` 的最小化/最大化/关闭语义）。主窗标题栏右侧的**设置齿轮直接复用 `titlebar_button()`**，所以标题栏按钮要改外观只需动这一处；可变的只有三样：`id`、悬停底色与图标字号（控制键 12、功能键 14）。其中 `format_span` 负责把秒数写成「1 天 2 小时 15 分 30 秒」，`format_day_label` 负责把日期转成「昨天 / 前天 / N 天前」。

`palette` 是全部界面颜色的唯一来源。因为 `gpui_kit::rgb()` 不是 `const fn`，无法定义 `const Rgba`，所以这里存原始 `u32`，使用处统一写 `rgb(palette::ACCENT)`；浮层那层半透明遮罩是 `hsla`，单独提供 `palette::overlay_bg()`。改主题只需动这一个模块。

**5. 平台适配层（`platform.rs`）**

所有 `unsafe` 的 Win32 调用集中于此，且全部提供非 Windows 空实现，保持上层代码零 `cfg` 分支：

- 对外接口一律接收 `&gpui_kit::Window`（只有 `show_main_window` 需要 `&mut`，以便在显示前置脏），原生 HWND 的提取（`HasWindowHandle` → `RawWindowHandle::Win32`）封在内部，调用方不再出现 `raw_window_handle` 依赖与 `cfg` 块
- `style_main_window` / `style_reminder_window` — 通过 DWM 设置窗口圆角（浮层直角、主窗圆角）并去掉系统描边；两者共用同一个 `set_window_chrome`，只差圆角常量
- `enable_system_menu_theme` — 调用 uxtheme 未公开导出 `SetPreferredAppMode`（序号 135）让原生菜单跟随系统暗色主题
- `set_autostart` — 注册表 Run 键的增删
- `ensure_single_instance` — `CreateMutexW` + `ERROR_ALREADY_EXISTS` 判定；句柄刻意不关闭，进程存活期间持续持有互斥体
- `show_main_window` — 按需 `SW_SHOWMAXIMIZED` / `SW_SHOWNOACTIVATE` 并置前；**置脏与显示绑在一起**（先 `Window::refresh()` 再 `ShowWindow`），调用方无法漏掉这一步
- `hide_main_window` — `ShowWindow(SW_HIDE)`：把窗口从屏幕上撤下但不销毁，HWND、渲染器与窗口内状态全部保留，托盘再次唤回时无需重建

### 一次提醒的完整数据流

```
调度线程 deadline 到期
      └─> event_tx.send(SchedulerEvent::Remind { remaining })
            └─> 转发线程 → AppEvent::Alarm → GPUI 主线程 match
                  └─> reminder_window::open_reminder_window()
                        用户点「喝了」
                          ├─> data::save_time()            写 SQLite + 增量入当天桶
                          ├─> scheduler_tx.send(SchedulerCmd::Reschedule(Drink))  deadline = now + interval
                          └─> window.remove_window()       关闭浮层
```

---

## 数据与配置位置

| 内容 | 路径 |
| --- | --- |
| 设置、窗口状态 | `%APPDATA%\water-remainder\settings.txt` |
| 喝水记录 | `%APPDATA%\water-remainder\data.db`（含 `-wal` / `-shm`） |
| 开机启动项 | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\WaterRemainder` |

---

## 构建与运行

```bash
# 调试运行
cargo run

# 发布构建（opt-level="z" + fat LTO + strip，体积优先）
cargo build --release
```

发布产物：`target\release\water-remainder.exe`

仓库内提供了打包脚本 `build-copy.ps1`：执行 release 构建后把 exe 覆盖复制到 `D:\executable`，方便直接分发。

## 源码结构

```
src/
├── main.rs              入口、单实例、事件汇聚循环
├── config.rs            设置模型与 settings.txt 读写、间隔常量表
├── paths.rs             应用数据目录（%APPDATA%\water-remainder）
├── data.rs              SQLite 连接池、天级聚合缓存 + 按需明细、记录读写
├── scheduler.rs         调度线程与 SchedulerCmd / SchedulerEvent 协议
├── tray.rs              托盘图标与右键菜单
├── ui.rs                时间工具、palette 配色、标题栏与共用按钮样式
├── main_window.rs       主窗口：日历热力图 + 时间轴
├── reminder_window.rs   全屏提醒浮层
├── settings_window.rs   设置窗口
├── platform.rs          Win32 / DWM 适配
└── assets/              water.ico / water.png
```

## 说明

- 目前仅 Windows 平台做了完整适配（托盘、注册表自启、DWM 样式）；非 Windows 下 `platform.rs` 为空实现，可编译运行但样式与自启能力受限。
- UI 依赖只声明 `gpui-kit` 一项，不再直接依赖 Zed 主仓。`gpui-kit` 是 longbridge 基于 GPUI 的组件库，它内部依赖把 Zed 的 `gpui` 重新发布的 crates.io 快照 `gpui-pre`（本版本对应 `zed@d89e9c2`），并把 gpui 全部根命名空间重导出——`gpui_kit::*` 就是 gpui，`gpui_kit::platform` 就是 `gpui_platform`。
  - 收益：不再需要 git checkout、版本可锁定、`cargo fetch` 走镜像。
  - 代价：底层 rev 由 `gpui-kit` 决定，上游变更只能等它跟进；且 `gpui-kit` 固定为 `gpui_platform` 打开 `font-kit` / `x11` / `wayland` / `runtime_shaders`，为 `gpui-pre` 打开 `windows-manifest`，这些特性无法从本项目侧关闭。
- `default-features = false` 关掉了 gpui-kit 的 `component` 与 `assets`：界面继续完全由本项目自绘（`palette` + `ui.rs` 的标题栏部件），不引入 shadcn 主题，也不嵌入图标资源。
- 启动时调用一次 `gpui_kit::init(cx)`（gpui-kit 的契约）。当前只启用 gpui 层，它实际只登记了 `gpui-base` 的主题与各控件全局态，本项目界面自绘、不读这些全局量。
