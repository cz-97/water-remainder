# water-remainder

使用 [GPUI](https://github.com/zed-industries/zed)（Zed 编辑器的 GPU 渲染 UI 框架）构建的 Windows 喝水提醒工具。常驻托盘、定时弹窗提醒，并记录每一次喝水打卡，形成日历热力图与时间轴。

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

---

## 技术栈

| 领域 | 选型 |
| --- | --- |
| 语言 | Rust 2024 edition |
| UI 渲染 | `gpui` + `gpui_platform`（git 依赖 Zed 主仓） |
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
   │ 菜单项   │   │ AppCmd 协议│ │ settings  │  │ 记录 + 缓存 │
   └────┬────┘   └─────┬─────┘  └───────────┘  └─────┬──────┘
        │              │                              │
        │   TrayIconEvent / MenuEvent / AppCmd          │
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

程序的唯一「装配点」。启动时先做单实例互斥检查，随后以 `QuitMode::Explicit` 启动 GPUI（窗口关闭不等于退出，保证托盘常驻）。

三个异构事件源被统一包装成 `AppEvent` 枚举，通过一条 `futures_channel::mpsc` 无界通道送入同一个异步消费任务：

- `TrayIconEvent`（托盘点击）— 来自 `tray-icon` 的全局回调
- `MenuEvent`（菜单项）— 来自 `muda` 的全局回调
- `AppCmd`（调度器信号）— 由独立的 OS 线程 `recv` 后转发

这样做的好处是：UI 的所有变更都串行发生在 GPUI 主线程的同一处 `match`，无需在回调里做跨线程的界面操作。

**2. 调度层（`scheduler.rs`）**

一个专用的 OS 线程，持有 `std::sync::mpsc` 双向通道：

- 输入 `AppCmd`：`Reschedule(Drink | Wake | ChangeInterval)`、`Trigger`、`Stop`
- 输出 `AppCmd::ShowOverlay(remaining_secs)`：交给主线程弹窗

核心是一个 **1 秒粒度的轮询循环**（`recv_timeout` 兼作 sleep），用 `deadline: SystemTime` 表示下次提醒时刻。超时即触发提醒并把 deadline 顺延；`Trigger` 则立即触发但不重置 deadline（因此浮层上能显示「将于 X 分钟后再次提醒」）。

`get_deadline()` 会把「距上次喝水已过去的时间」从间隔中扣除，所以重启程序或从睡眠唤醒后，提醒节奏仍然连续。

**3. 存储层**

拆成两个互不干扰的持久化通道：

- `config.rs` — 设置与窗口状态，纯文本 `key=value`，路径 `%APPDATA%\water-remainder\settings.txt`。零依赖、可读、损坏时逐行回退默认值。`INTERVALS` 常量表用宏生成 `(秒, "N 分钟")` 二元组，UI 与调度共用同一份数据源。
- `data.rs` — 喝水记录，SQLite 单表 `drink_records(id, timestamp)` + `timestamp` 索引。开启 WAL 与 `busy_timeout`，`r2d2` 连接池上限 4。记录以 **`DrinkCache`（`BTreeMap<NaiveDate, Vec<u64>>`，按本地日期分桶、桶内升序）做进程内全量缓存**：首次 `snapshot()` 读取一次数据库并完成分桶，之后渲染只走内存；`save_time()` 落库成功后把新时间戳增量塞进所属日期的桶。提醒文案与调度需要的「上次喝水距今」也由 `get_elapsed()` 从这份缓存取（`last_time()` = `BTreeMap` 尾桶的尾元素），不再单独维护一份时间戳。

**记录缓存的生命周期（`data.rs`）**

`RECORDS` 是唯一的数据源，`LAST_TIME` 之类的旁路缓存已移除。进程运行期间与数据库只有两类交互：

| 时机 | 数据库操作 |
| --- | --- |
| 首次 `snapshot()`（进程内第一次读记录，含调度线程 `get_deadline()` → `get_elapsed()` 那次） | 一次 `SELECT * ORDER BY timestamp` 全量读取 |
| 每次点击「喝了」 | 一次 `INSERT` |

此后无论主窗重绘多少次——**GPUI 每次 draw 都会重跑 `render()`，没有「状态未变就跳过」的优化**——日历热力图与时间轴都直接命中内存缓存，不再产生任何 `SELECT`。单次 `snapshot()` 只是 `Arc` 克隆 + 一次读锁；日历格子取 `count(day)`、点击某天后取 `times(day)`，都是 `BTreeMap` 查找，不拷贝数据。

代价与边界：缓存不做失效，若程序运行期间由外部直接改写 `data.db`，界面不会感知（重启进程即重新加载）。

**4. UI 层**

三个独立的 GPUI 窗口，各自是实现了 `Render` 的 View：

- `main_window.rs` — 记录总览。日历按「今天向前倒推」逐行生成（每行 7 天，行内首次遇到 `day == 1` 时标注月份），配色由 `ui::calendar_color()` 按当日次数映射到蓝色梯度，形成 GitHub 贡献图式热力图；右侧是选中日期的时间轴，今天额外显示「N 分钟前」相对时间。
  - `MainWindow` 结构体只有 `store` / `scheduler` / `selected_date` 三个字段，**不持有任何记录数据**。渲染时只为每个格子读一次 `cache.count(day)`（返回 `usize`）上色；选中日期的明细列表则按 `selected_date` 现取 `cache.times(day)`（借用切片，不拷贝），点某天即改 `selected_date` 并 `cx.notify()` 触发新一轮 `render`。
- `reminder_window.rs` — 覆盖整个主显示器的透明 `WindowKind::PopUp` 浮层。文案不预先生成，而是把 `last_drink` / `next_reminder` 两个时间戳存进 View，渲染时按「当前时刻」现算，因此有几点行为：
  - **两处倒计时按秒跳动**。实体创建时 `cx.spawn` 起一个 1 秒周期的后台定时任务，每轮醒来 `cx.notify()` 触发重绘（`notify` → `invalidate_view` 置窗口 dirty 并唤醒平台 waker → 下一帧重跑 `render`）。窗口关闭后弱引用升级失败，任务自行退出，不会泄漏。
  - 文案形如 `您在 1 天 2 小时 15 分 30 秒前喝过水（昨天 15:04:32），将于 12 分 3 秒后再次提醒您（15:37:11）`。时间一律写到秒，并用「从最大非零单位一路展开到秒」的写法，避免出现「1 小时 59 秒」这种有歧义的省略。
  - 括号内是具体时刻 `HH:MM:SS`；日期用相对词表示——当天省略、`昨天`、`前天`、`N 天前`。下次提醒的时刻始终不写日期。
  - 「喝了」写入记录并 `Reschedule(Drink)`，「跳过」仅关闭窗口。
- `settings_window.rs` — 间隔步进器（受 `INTERVALS` 边界约束，越界时按钮置灰）+ 开机启动开关。修改间隔会同时落盘、改注册表、并向调度器发送 `ChangeInterval`。

`ui.rs` 是共享工具模块：时间戳换算（`now` / `local_date` / `format_clock` / `format_clock_secs` / `format_span` / `format_day_label` / `relative_to_now`）、热力图调色板、以及自绘标题栏按钮 `window_button`（用 `Segoe Fluent Icons` 字体 + `WindowControlArea` 实现拖拽与最小化/最大化/关闭）。其中 `format_span` 负责把秒数写成「1 天 2 小时 15 分 30 秒」，`format_day_label` 负责把日期转成「昨天 / 前天 / N 天前」。

**5. 平台适配层（`platform.rs`）**

所有 `unsafe` 的 Win32 调用集中于此，且全部提供非 Windows 空实现，保持上层代码零 `cfg` 分支：

- `strip_win11_chrome` / `style_main_window` — 通过 DWM 设置窗口圆角（浮层直角、主窗圆角）并去掉系统描边
- `enable_system_menu_theme` — 调用 uxtheme 未公开导出 `SetPreferredAppMode`（序号 135）让原生菜单跟随系统暗色主题
- `set_autostart` — 注册表 Run 键的增删
- `ensure_single_instance` — `CreateMutexW` + `ERROR_ALREADY_EXISTS` 判定
- `show_main_window` — 按需 `SW_SHOWMAXIMIZED` / `SW_SHOWNOACTIVATE` 并置前

### 一次提醒的完整数据流

```
调度线程 deadline 到期
      └─> at.send(AppCmd::ShowOverlay(secs))
            └─> 转发线程 → AppEvent::Alarm → GPUI 主线程 match
                  └─> reminder_window::open_reminder_window()
                        用户点「喝了」
                          ├─> data::save_time()            写 SQLite + 增量入当天桶
                          ├─> scheduler_tx.send(Reschedule(Drink))  deadline = now + interval
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
├── data.rs              SQLite 连接池、DrinkCache 记录缓存、记录读写
├── scheduler.rs         调度线程与 AppCmd 协议
├── tray.rs              托盘图标与右键菜单
├── ui.rs                时间工具、热力图配色、标题栏按钮
├── main_window.rs       主窗口：日历热力图 + 时间轴
├── reminder_window.rs   全屏提醒浮层
├── settings_window.rs   设置窗口
├── platform.rs          Win32 / DWM 适配
└── assets/              water.ico / water.png
```

## 说明

- 目前仅 Windows 平台做了完整适配（托盘、注册表自启、DWM 样式）；非 Windows 下 `platform.rs` 为空实现，可编译运行但样式与自启能力受限。
- `gpui` 依赖 Zed 主仓的最新提交，首次构建耗时较长，且 API 存在上游变动风险。
