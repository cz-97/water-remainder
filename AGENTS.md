# AGENTS.md

本文件是本项目的 Agent 工作约定与实现说明。在此仓库中修改代码时，请遵循以下流程。

## 改动后的固定流程

每次修改代码后，**按顺序**执行：

1. **编译检查** — `cargo check --offline`
   - 失败就地修复并重新检查，**通过之前不要进入后续步骤**。
2. **运行验证** — `cargo run`
   - 仅在第 1 步通过后执行。
   - 这是常驻托盘的 GUI 程序（单实例、命名互斥体）：若旧进程仍在运行，**先结束它**再 `cargo run`，否则新进程会因互斥体已存在而直接退出。结束命令：`taskkill /F /IM water-remainder.exe`。
   - 启动后**不要结束进程**，让它继续常驻托盘运行。
   - 若依赖未缓存，联网受限时改用 `cargo run --offline`。
3. **提交** — `git commit`
   - 仅在第 1 步通过后执行；先暂存本次改动再提交。
   - 提交信息遵循下方规范（与个人 `AGENTS.md` 一致）；**不需要展示提交细节**（文件清单、diff、commit hash 等）。

一句话：**`cargo check --offline` 通过 → 先结束旧进程再 `cargo run`（启动后保留运行）→ `git commit`。**

## 提交信息规范

- 首行控制在 50 字符内，祈使句、首字母大写、结尾不加标点。
- 需要时用空行分隔正文，正文每行 ≤ 72 字符，简短说明「为什么」而非「做了什么」。
- 正文不要重复首行信息；若首行已说清则省略正文。

## 注意事项

- `target/` 已在 `.gitignore` 中，不要提交构建产物。
- 沙箱内 `git commit` 需要写 Git 元数据；若被拒绝，请申请以 unsandboxed 方式执行。
- 不要为了让流程通过而放宽检查或注释掉代码；编译失败必须定位到根因。

---

# 技术细节

本项目的实现说明（原先写在 README，现集中于此）。

## 技术栈

| 领域 | 选型 |
| --- | --- |
| 语言 | Rust 2024 edition |
| UI 渲染 | `gpui-kit`（启用 `component` / `assets`：gpui-component 的 shadcn 风格控件 + Lucide 图标；底层 gpui / gpui_platform） |
| 托盘与菜单 | `tray-icon` / `muda` |
| 窗口句柄 | `raw-window-handle` + `windows` crate (Win32 / DWM) |
| 时间处理 | `chrono`（clock 特性） |
| 存储 | `rusqlite`（bundled SQLite）+ `r2d2` / `r2d2_sqlite` 连接池 |
| 跨线程通信 | `std::sync::mpsc`（调度线程）+ `futures_channel`（GPUI 事件循环） |
| 资源嵌入 | `embed-resource` + `.rc` 文件（图标） |

## 架构总览

> 分类后源码按职责归入 `core/`（配置、存储、调度等逻辑）、`ui/`（窗口与共享部件）、`platform/`（系统适配）三个目录，图中为模块名，完整路径见「源码结构」。

### 分层说明

**1. 入口与事件汇聚层（`main.rs`）**

程序的唯一「装配点」。启动时先做单实例互斥检查，随后 `gpui_kit::init(cx)` 初始化 gpui-kit 已启用的层，再以 `QuitMode::Explicit` 启动 GPUI——关闭窗口不等于结束进程（主窗关闭只是隐藏），托盘常驻，退出走托盘菜单。

三个异构事件源被统一包装成 `AppEvent` 枚举，通过一条 `futures_channel::mpsc` 无界通道送入同一个异步消费任务：

- `TrayIconEvent`（托盘点击）— 来自 `tray-icon` 的全局回调
- `MenuEvent`（菜单项）— 来自 `muda` 的全局回调
- `SchedulerEvent`（调度器信号）— 由独立的 OS 线程 `recv` 后转发

这样做的好处是：UI 的所有变更都串行发生在 GPUI 主线程的同一处 `match`，无需在回调里做跨线程的界面操作。

这里还订阅了一次系统唤醒（`cx.on_system_wake`）：唤醒后向调度器发 `Reschedule(Wake)`，把 deadline 按「距上次喝水已过去多久」重算一次。**这个 `Subscription` 必须 `detach()`**——`Platform::run` 是先调用启动闭包、之后才进入 `GetMessageW` 消息循环（`gpui-pre-windows/src/platform.rs:508`），绑在闭包里的 RAII 守卫会在消息循环开始之前就被 drop 掉并注销回调，回调于是永远不会被调用。同一处的 `std::mem::forget(tray)` 处理的是同一个「闭包提前返回」问题。

**2. 调度层（`core/scheduler.rs`）**

一个专用的 OS 线程，持有 `std::sync::mpsc` 双向通道：

- 输入 `SchedulerCmd`：`Reschedule(Drink | Wake | ChangeInterval)`、`Trigger`、`Stop`
- 输出 `SchedulerEvent::Remind { remaining }`：交给主线程弹窗

两个方向各自一个枚举。这样 UI 侧的 `match` 只面对 `Remind` 一个变体，不需要用 catch-all 分支去吞掉「只发给调度器的命令」——将来调度器新增事件时，编译器的穷尽性检查会直接报出未处理的变体，而不是被静默忽略。

核心是一个 **1 秒粒度的轮询循环**（`recv_timeout` 兼作 sleep），用 `deadline: SystemTime` 表示下次提醒时刻。超时即触发提醒并把 deadline 顺延；`Trigger` 则立即触发但不重置 deadline（因此浮层上能显示「将于 X 分钟后再次提醒」）。

`get_deadline()` 会把「距上次喝水已过去的时间」从间隔中扣除，所以重启程序或从睡眠唤醒后，提醒节奏仍然连续。

**3. 存储层**

拆成两个互不干扰的持久化通道：

- `core/config.rs` — 设置与窗口状态，纯文本 `key=value`，路径 `%APPDATA%\water-remainder\settings.txt`。零依赖、可读、损坏时逐行回退默认值。`INTERVALS: &[u64]` 是间隔秒数表，设置窗按索引取值、标签由秒数现算「N 分钟」，UI 与调度共用同一份数据源。
- `core/data.rs` — 喝水记录，SQLite 单表 `drink_records(id, timestamp)` + `timestamp` 索引。开启 WAL 与 `busy_timeout`，`r2d2` 连接池上限 4。内存按需分层：**按月懒加载的日级聚合 `MONTHS`（`BTreeMap<月, DayCounts{日期→次数}>`，只在访问某月时查一次，之后含空月永久命中）** 与 **明细 `DETAILS`（`BTreeMap<NaiveDate, Arc<Vec<u64>>>`，点开某天才查一次）**；另有独立的全局极值 `BOUNDS`（最早 / 最近时间戳，供左箭头下界与「上次喝水」）。`save_time()` 落库后只增量更新这些缓存。`get_elapsed()` 由 `last_time()`（全局 `MAX(timestamp)`）得出，**完全不需要明细**。

**记录缓存的生命周期（`core/data.rs`）**

`MONTHS`（按月聚合，按需）、`DETAILS`（明细，按需）与 `BOUNDS`（全局极值）是记录的唯一来源。进程运行期间与数据库只有这几类交互：

| 时机 | 数据库操作 |
| --- | --- |
| 首次访问某月（启动首帧只访问当月；之后每次切月各访问一次） | 一次区间聚合 `SELECT date(…,'localtime') AS day, COUNT(*) … WHERE timestamp >= ?1 AND timestamp < ?2 GROUP BY day` —— **一个月一行天，命中 `idx_timestamp`，不读明细** |
| 首次需要「上次喝水 / 最早记录」（调度线程 `get_deadline()`、左箭头下界、提醒浮层） | 一次 `SELECT MIN(timestamp), MAX(timestamp) FROM drink_records` —— 走 `idx_timestamp`，代价与记录数无关 |
| 首次点开某天，且那天次数 > 0（`day_detail()`） | 一次区间查询 `WHERE timestamp >= ?1 AND timestamp < ?2`，命中 `idx_timestamp` 做索引查找 |
| 每次点击「喝了」 | 一次 `INSERT` |

三个刻意的取舍：

- **选中日的次数为 0 时一条 SQL 都不发**：`day_detail()` 先查该日所在月的聚合，为 0 就直接返回空 `Arc`，连缓存条目都不建。
- **某天的明细一天最多查一次**：结果进 `DETAILS`，此后（含窗口重绘）都命中内存。`save_time()` 也只在「那天的明细已经加载过」时才把新时间戳追加进已缓存的那条，**绝不因为写入而顺带加载明细**。
- **三层缓存都只增不减、不做失效**：若程序运行期间由外部直接改写 `data.db`，界面不会感知（重启进程即重新加载）。

于是 GPUI「每次 draw 都重跑 `render()`」不再有代价：日历每格是 `counts.count(day)`（`BTreeMap` 查找），时间轴是选中日明细的 `Arc` 克隆，都不产生 `SELECT`（切月仅在该月首次访问时多一次走索引的月度区间查询）。代价是日期边界要由 chrono 自己算（`ui::day_bounds()` 给出该本地日的 `[00:00, 次日 00:00)`），换来的是「按天取明细」永远是一次索引区间查找 —— 比按 `date()` 函数过滤快近两个数量级，也不必给表加 `day` 列（SQLite 不允许生成列里用 `localtime`，`INSERT` 会直接报非确定性错误）。

**4. UI 层**

三个独立的 GPUI 窗口，各自是实现了 `Render` 的 View：

- `ui/main_window.rs` — 记录总览。日历为月视图：标题是「年 月」，左右箭头切换 `view_month`（右箭头逐月前进、不允许翻到未来；左箭头逐月后退、退到 `data::earliest_day`（由全局 `MIN(timestamp)` 得出）所在的最早记录月为止），每周以周一为首列；每格是当月某天，背景统一由 `ui::calendar_color()` 按当日次数映射到蓝色梯度（0 次保留色阶底色的深灰格，浅色格用深色数字保证可读），选中日加白框。右侧是选中日期的时间轴，今天额外显示「N 分钟前」相对时间。
  - `MainWindow` 结构体只有 `store` / `scheduler` / `selected_date` / `view_month` 四个字段，**不持有任何记录数据**。渲染时只为每个格子读一次 `counts.count(day)`（返回 `usize`）上色；右侧明细先经 `day_detail(selected)` 取到（懒加载 + 缓存），再按 `Arc` 借用渲染；点某天即改 `selected_date` 并 `cx.notify()` 触发新一轮 `render`。
  - `render()` 只做三件事：组装一次性的 `RenderInput`（`today` / `selected` / `earliest` / 当月聚合 / 选中日明细）、调用三个子视图、拼外壳。时刻与记录因此是**显式传参**，`calendar()` / `timeline()` / `title_actions()` 不会各自去读全局状态。三个子视图的返回类型写成具体的 `Div` 而非 `impl IntoElement`，否则返回值会隐式借用 `&mut Context`，同一个 `render` 里就没法把 `cx` 依次交给多个子视图。
  - 关闭按钮**不销毁窗口**：`on_window_should_close` 采集并落盘窗口状态后调 `platform::hide_main_window()`，返回 `false` 让 gpui 吞掉 `WM_CLOSE`（不再交给 `DefWindowProc` 销毁）。隐藏期间窗口收不到 `WM_PAINT`，也没有任何路径会 `notify` 它——**记录入库只写缓存、不通知主窗**，所以日历的刷新完全依赖 `show_main_window` 里那次置脏：唤回时重绘一帧，重新取聚合与选中日明细（都命中缓存）+ `local_date(now())`。
- `ui/reminder_window.rs` — 覆盖整个主显示器的透明 `WindowKind::PopUp` 浮层。文案不预先生成，而是把 `last_drink` / `next_reminder` 两个时间戳存进 View，渲染时按「当前时刻」现算，因此有几点行为：
  - **两处倒计时按秒跳动**。实体创建时 `cx.spawn` 起一个 1 秒周期的后台定时任务，每轮醒来 `cx.notify()` 触发重绘（`notify` → `invalidate_view` 置窗口 dirty 并唤醒平台 waker → 下一帧重跑 `render`）。窗口关闭后弱引用升级失败，任务自行退出，不会泄漏。
  - 文案形如 `您在 1 天 2 小时 15 分 30 秒前喝过水（昨天 15:04:32），将于 12 分 3 秒后再次提醒您（15:37:11）`。时间一律写到秒，并用「从最大非零单位一路展开到秒」的写法，避免出现「1 小时 59 秒」这种有歧义的省略。
  - 括号内是具体时刻 `HH:MM:SS`；日期用相对词表示——当天省略、`昨天`、`前天`、`N 天前`。下次提醒的时刻始终不写日期。
  - 「喝了」写入记录并 `Reschedule(Drink)`，「跳过」仅关闭窗口。
  - **中央插图可换**：默认用内置 `water.png`；若数据目录下存在 `reminder.img`（由设置窗「更换图片」写入）则优先用它，按文件头魔数解码（png / jpg / gif / bmp / webp / tif / ico），无法解析时回退内置图。
  - 浮层已经存在时**不静默返回**，而是原地更新 `last_drink` / `next_reminder` 并重绘。浮层有可能是在屏幕没亮、显示器正在重枚举的那个瞬间被创建出来的——用户看不见它，而它除了被点击之外不会被自动关闭，静默返回会让此后每一次提醒都被吞掉，直到重启进程。
- `ui/settings_window.rs` — 控件均来自 gpui-component：间隔步进器（`Button`，受 `INTERVALS` 边界约束，越界时置灰）+ 开机启动开关（`Switch`）+ 提醒图片「更换 / 恢复默认」（`Button`，无自定义图时「恢复默认」置灰）。间隔与开机启动走 `config::update_settings(&store, |settings| ...)`：加锁、改值、落盘收在这一个函数里；图片更换则把选中的图片复制到 `core::paths::reminder_image_file()`（仅识别得到格式的图片，取消或非图片则不改动），恢复默认即删除该文件。间隔变更随后向调度器发送 `ChangeInterval`。

`ui/mod.rs` 是共享工具模块：时间戳换算（`now` / `local_date` / `format_clock` / `format_clock_secs` / `format_span` / `format_day_label` / `relative_to_now`）、配色表 `palette`、热力图取色 `calendar_color`、图片格式探测 `image_format`。窗口的按钮 / 开关 / 标题栏已改用 gpui-component（`Button` / `Switch` / `TitleBar`），此前的自绘标题栏部件（`titlebar` / `titlebar_button` / `window_button`）已移除。其中 `format_span` 负责把秒数写成「1 天 2 小时 15 分 30 秒」，`format_day_label` 负责把日期转成「昨天 / 前天 / N 天前」。

`palette` 现只服务自绘部分（日历热力图、提醒浮层遮罩与少量文字色）；其余控件颜色来自 gpui-component 的主题。浮层那层半透明遮罩是 `hsla`，单独提供 `palette::overlay_bg()`。

**5. 平台适配层（`platform/`）**

所有 `unsafe` 的 Win32 调用集中于此，且全部提供非 Windows 空实现，保持上层代码零 `cfg` 分支：

- 对外接口一律接收 `&gpui_kit::Window`（只有 `show_main_window` 需要 `&mut`，以便在显示前置脏），原生 HWND 的提取（`HasWindowHandle` → `RawWindowHandle::Win32`）封在内部，调用方不再出现 `raw_window_handle` 依赖与 `cfg` 块
- `style_main_window` / `style_reminder_window` — 通过 DWM 设置窗口圆角（浮层直角、主窗圆角）并去掉系统描边；两者共用同一个 `set_window_chrome`，只差圆角常量
- `enable_system_menu_theme` — 调用 uxtheme 未公开导出 `SetPreferredAppMode`（序号 135）让原生菜单跟随系统暗色主题
- `set_autostart` — 注册表 Run 键的增删
- `ensure_single_instance` — `CreateMutexW` + `ERROR_ALREADY_EXISTS` 判定；句柄刻意不关闭，进程存活期间持续持有互斥体
- `pick_image_file` — `GetOpenFileNameW` 弹出系统「打开文件」对话框（`Win32_UI_Controls_Dialogs` feature），供设置窗「更换图片」用
- `show_main_window` — 按需 `SW_SHOWMAXIMIZED` / `SW_SHOWNOACTIVATE` 并置前；**置脏与显示绑在一起**（先 `Window::refresh()` 再 `ShowWindow`），调用方无法漏掉这一步
- `hide_main_window` — `ShowWindow(SW_HIDE)`：把窗口从屏幕上撤下但不销毁，HWND、渲染器与窗口内状态全部保留，托盘再次唤回时无需重建

### 一次提醒的完整数据流

```
调度线程 deadline 到期
      └─> event_tx.send(SchedulerEvent::Remind { remaining })
            └─> 转发线程 → AppEvent::Alarm → GPUI 主线程 match
                  └─> ui::reminder_window::open_reminder_window()
                        用户点「喝了」
                          ├─> core::data::save_time()            写 SQLite + 增量入当天桶
                          ├─> scheduler_tx.send(SchedulerCmd::Reschedule(Drink))  deadline = now + interval
                          └─> window.remove_window()       关闭浮层
```

## 数据与配置位置

| 内容 | 路径 |
| --- | --- |
| 设置、窗口状态 | `%APPDATA%\water-remainder\settings.txt` |
| 喝水记录 | `%APPDATA%\water-remainder\data.db`（含 `-wal` / `-shm`） |
| 自定义提醒图片 | `%APPDATA%\water-remainder\reminder.img`（存在即启用，删除即恢复内置图） |
| 开机启动项 | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\WaterRemainder` |

## 源码结构

```
src/
├── main.rs                  入口、单实例、事件汇聚循环
├── core/                    领域逻辑（配置、存储、调度，无 UI 依赖）
│   ├── config.rs            设置模型与 settings.txt 读写、间隔常量表
│   ├── data.rs              SQLite 连接池、按月聚合 + 按需明细、记录读写
│   ├── paths.rs             应用数据目录（%APPDATA%\water-remainder）
│   └── scheduler.rs         调度线程与 SchedulerCmd / SchedulerEvent 协议
├── ui/                      界面（GPUI View 与共享部件）
│   ├── mod.rs               时间工具、palette 配色、热力图取色、图片格式探测
│   ├── main_window.rs       主窗口：月历 + 时间轴
│   ├── reminder_window.rs   全屏提醒浮层
│   └── settings_window.rs   设置窗口
├── platform/                系统适配
│   ├── mod.rs               Win32 / DWM 适配
│   └── tray.rs              托盘图标与右键菜单
└── assets/                  water.ico / water.png
```

## 构建与运行

```bash
# 调试运行
cargo run

# 发布构建（opt-level="z" + fat LTO + strip，体积优先）
cargo build --release
```

发布产物：`target\release\water-remainder.exe`

仓库内提供了打包脚本 `build-copy.ps1`：执行 release 构建后把 exe 覆盖复制到 `D:\executable`，方便直接分发。

## 说明

- 目前仅 Windows 平台做了完整适配（托盘、注册表自启、DWM 样式）；非 Windows 下 `platform.rs` 为空实现，可编译运行但样式与自启能力受限。
- UI 依赖只声明 `gpui-kit` 一项，不再直接依赖 Zed 主仓。`gpui-kit` 是 longbridge 基于 GPUI 的组件库，它内部依赖把 Zed 的 `gpui` 重新发布的 crates.io 快照 `gpui-pre`（本版本对应 `zed@d89e9c2`），并把 gpui 全部根命名空间重导出——`gpui_kit::*` 就是 gpui，`gpui_kit::platform` 就是 `gpui_platform`。
  - 收益：不再需要 git checkout、版本可锁定、`cargo fetch` 走镜像。
  - 代价：底层 rev 由 `gpui-kit` 决定，上游变更只能等它跟进；且 `gpui-kit` 固定为 `gpui_platform` 打开 `font-kit` / `x11` / `wayland` / `runtime_shaders`，为 `gpui-pre` 打开 `windows-manifest`，这些特性无法从本项目侧关闭。
- 已启用 gpui-kit 的 `component`（gpui-component，shadcn 风格控件）与 `assets`（Lucide 图标）：窗口的按钮 / 开关 / 标题栏改用组件库（`Button` / `Switch` / `TitleBar`），日历热力图与时间轴仍自绘（`palette`）。
- **依赖坑**：`gpui-component 0.6.1` 与最新的 `gpui-component-macros 0.6.6` 不兼容（`IntoPlot` 派生找不到 `plot::tooltip::track_hover`，组件自身编译失败）。`Cargo.lock` 未纳入版本控制，故 `Cargo.toml` 已把 `gpui-component-macros` 钉在 `=0.6.1`；**不要放宽这个约束**，否则会解析到 0.6.6 导致编译失败。
- 启动时调用一次 `gpui_kit::init(cx)`（同时初始化 gpui-base 主题与组件全局态），并以 `.with_assets(gpui_kit::assets::Assets)` 挂上图标资源。
