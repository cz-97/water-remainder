# AGENTS.md

本文件是本项目的 Agent 工作约定与实现说明。在此仓库中修改代码时，请遵循以下流程。

## 改动后的固定流程

每次修改代码后，**按顺序**执行：

1. **编译检查** — `cargo check --offline`
   - 失败就地修复并重新检查，**通过之前不要进入后续步骤**。
   - 同时跑 `cargo test --offline`：纯函数（分档、格式化、本地日区间、设置解析与原子写）都有测试，改了这些地方必须让测试保持通过。
   - 再跑 `cargo clippy --offline`：`Cargo.toml` 的 `[lints.clippy]` 把 `clippy::all` 提成了 `deny`，**新增警告会直接让 clippy 失败**（已实测：注入一个 `len_zero` 会让 exit code 变 101）。
   - `cargo fmt --check` 应为空。工具链由 `rust-toolchain.toml` 钉在 `stable` 通道（**不要**改成精确版本号，那样 rustup 会去联网下载特定版本，本地没装过就直接失败）。
2. **运行验证** — `cargo run`
   - 仅在第 1 步通过后执行。
   - 这是常驻托盘的 GUI 程序（单实例、命名互斥体）：若旧进程仍在运行，**先结束它**再 `cargo run`，否则新进程会因互斥体已存在而直接退出。结束命令：`taskkill /F /IM water-remainder.exe`。
   - 启动后**不要结束进程**，让它继续常驻托盘运行。
   - 若依赖未缓存，联网受限时改用 `cargo run --offline`。
3. **提交** — `git commit`
   - 仅在第 1 步通过后执行；先暂存本次改动再提交。
   - **提交前先按下方规则决定是否更新 `Cargo.toml` 的 `version`**，需要时连同版本号一起暂存。
   - 提交信息遵循下方规范（与个人 `AGENTS.md` 一致）；**不需要展示提交细节**（文件清单、diff、commit hash 等）。

一句话：**`cargo check --offline` 通过 → 先结束旧进程再 `cargo run`（启动后保留运行）→ 按需改版本号 → `git commit`。**

## 版本号规则

本项目遵循语义化版本 `MAJOR.MINOR.PATCH`（Cargo 的默认约定）：

| 段 | 什么时候递增 | 典型情况 |
| --- | --- | --- |
| `MAJOR` | 不兼容的变更 | 破坏用户数据格式、去掉功能、改掉对外契约 | 0.5.1 → 1.0.0 |
| `MINOR` | 向后兼容的新功能 | 新增设置项、新增窗口/视图、新增或修改可见界面行为 | 0.5.1 → 0.6.0 |
| `PATCH` | 向后兼容的缺陷修复 | 修 offset/缝隙、配色错值、崩溃、逻辑错误 | 0.5.1 → 0.5.2 |

执行要点：

- **一次 commit 最多跳一级**，不要连跳；多个同段变更在同一 commit 里只递增一次。
- 递增后**低位归零**：`0.5.1` 提 `MINOR` 得 `0.6.0`，提 `MAJOR` 得 `1.0.0`。
- `MAJOR` 为 `0` 表示尚不稳定，此时允许在 `0.x` 内因不兼容改动而递增 `MINOR`（Cargo 本身就是这样处理 `0.x` 的兼容段的）。
- **纯内部改动不升版本**：重构、注释、文档（含 `AGENTS.md`）、构建脚本、`.gitignore`，以及不影响用户可见行为的依赖调整。
- 判断依据写在大脑里即可，**不要为了凑版本号而把一次普通修复描述成新功能**。

近期改动按此规则应当为：主题跟随系统（新能力，`MINOR`）、铺满整屏与去边框（修缺陷，`PATCH`）。

## 提交信息规范

- 首行控制在 50 字符内，祈使句、首字母大写、结尾不加标点。
- 需要时用空行分隔正文，正文每行 ≤ 72 字符，简短说明「为什么」而非「做了什么」。
- 正文不要重复首行信息；若首行已说清则省略正文。

## 注意事项

- `target/` 已在 `.gitignore` 中，不要提交构建产物。
- 沙箱内 `git commit` 需要写 Git 元数据；若被拒绝，请申请以 unsandboxed 方式执行。
- 不要为了让流程通过而放宽检查或注释掉代码；编译失败必须定位到根因。

## 这是个桌面 GUI 程序，不是 Web 前端

**这是一个常驻托盘的 Windows 桌面程序**：有 DPI 缩放、系统主题、注册表、单实例互斥体、睡眠唤醒、显示器增删这些**真实系统状态**，没有浏览器 DevTools，也没有「刷新页面」这种重来一次的机会。做事方式必须相应调整：

- **分清「逻辑」与「系统行为」**。纯逻辑（分档、格式化、日期区间、设置解析）用 `cargo test` 验证，快且可靠 —— 优先把能测的部分做成纯函数。
- **系统行为不要靠猜，也不要靠反复试**。凡是涉及 DPI、多显示器、窗口层叠、注册表、托盘、电源事件的行为，**要么用系统 API 取到确凿数据再下结论，要么请人来看一眼**。
- **观测本身要先确认有效**。截图/枚举窗口这类探测，先确认探测进程自己的缩放与坐标语义（例如截图前是否 `SetProcessDpiAwarenessContext`），否则拿到的是虚拟化后的假数据 —— 本项目曾据此误报过一个并不存在的「浮层偏移」缺陷。
- **不要死磕，够几次就停下来问人**。同一个问题反复失败、或需要「等一段时间 / 等某个时机」才能观察到、或必须在真实交互下才能确认时，**不要继续轮询硬等或改配置去凑条件**，直接说明现象并请人确认。人看一眼（截个图、点一下托盘）通常比 agent 绕十圈都快。
- **利用现成的入口，不要自造场景**。运行时想看提醒浮层，就走托盘菜单的「立即提醒」（`SchedulerCmd::Trigger`，立即触发且不重置 deadline），**不要去改 `interval` 凑时间** —— `normalize_interval` 会把值收敛到档位，`get_deadline()` 又会从中扣掉已过去的时间，改配置很难凑出想要的效果，而且会动到用户的真实设置。
- **不要长期改动用户的本地状态**。`%APPDATA%\water-remainder\` 下的设置与记录是用户真实数据：临时改过必须还原，测试期间绝不能写入假记录。调试运行时启动会按 `autostart` 重写注册表 Run 键，改完记得恢复。

---

# 技术细节

本项目的实现说明（原先写在 README，现集中于此）。

## 技术栈

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

## 架构总览

> 分类后源码按职责归入 `core/`（配置、存储、调度等逻辑）、`ui/`（窗口与共享部件）、`platform/`（系统适配）三个目录，图中为模块名，完整路径见「源码结构」。

### 分层说明

**1. 入口与事件汇聚层（`main.rs`）**

程序的唯一「装配点」。启动时先做单实例互斥检查，随后 `gpui_kit::init(cx)` 初始化 gpui-kit 已启用的层，再以 `QuitMode::Explicit` 启动 GPUI——关闭窗口不等于结束进程（主窗关闭只是隐藏），托盘常驻，退出走托盘菜单。

**启动失败一律要「说话」**：本程序是 `windows_subsystem = "windows"` 的托盘程序，release 下又是 `panic = "abort"`，所以任何在这里 panic 都表现为「进程连同托盘图标一起无声消失」。`setup_tray()` 因此返回 `Result<_, String>` 而不是内部 `expect`，失败时走 `platform::fatal_startup_error(message)` —— 它弹一个原生 `MessageBoxW`（此时还没有任何 GPUI 窗口可用）再 `process::exit(1)`，返回类型是 `!`，调用方不必考虑「提示完还继续跑」的分支。判据是**只有真正导致不可用的问题才算致命**：托盘图标建不起来 → 既没有常驻入口也没有退出方式，致命；往右键菜单里追加条目的失败只是局部降级（图标与左键开会仍在），仍用 `.ok()` 忽略。

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

- `core/config.rs` — 设置与窗口状态，纯文本 `key=value`，路径 `%APPDATA%\water-remainder\settings.txt`。零依赖、可读、损坏时逐行回退默认值。`INTERVALS: &[u64]` 是间隔秒数表，设置窗按索引取值、标签由秒数现算「N 分钟」，UI 与调度共用同一份数据源。落盘走 `write_atomic`（同目录临时文件 + `sync_all` + 改名覆盖），因为启动时会无条件按 `autostart` 调 `set_autostart`，一次「写一半就崩溃」会让文件回退成默认值、进而**静默关掉用户的开机启动**。
  - **落盘失败必须能被调用方看见**：`save_store` 返回 `io::Result`，`update_settings` 返回 `Result<_, SaveError>`，且在失败时**把内存里的改动回滚**。否则界面显示「已开启」而文件里还是旧值，用户重启一次才发现设置莫名复原。
  - `settings_snapshot(&store)` 取设置快照，锁中毒时用 `into_inner()` 继续读而不 `unwrap`：`Settings` 是纯数据，中毒只是「有人持锁时 panic 过」，内容并没坏；而在启动路径上 panic 会无声退出（release 下 `panic = "abort"`）。
  - 只有**用户在意**的设置在失败时提示（`interval` / `autostart`）；窗口位置尺寸属于「记不住也不影响使用」，`save_store` 的返回值被刻意 `let _ =` 忽略，不值得弹框打扰。
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

于是 GPUI「每次 draw 都重跑 `render()`」不再有代价：日历每格是 `counts.count(day)`（`BTreeMap` 查找），时间轴是选中日明细的 `Arc` 克隆，都不产生 `SELECT`（切月仅在该月首次访问时多一次走索引的月度区间查询）。代价是日期边界要由 chrono 自己算（`core::time::day_bounds()` 给出该本地日的 `[00:00, 次日 00:00)`），换来的是「按天取明细」永远是一次索引区间查找 —— 比按 `date()` 函数过滤快近两个数量级，也不必给表加 `day` 列（SQLite 不允许生成列里用 `localtime`，`INSERT` 会直接报非确定性错误）。

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
- `ui/settings_window.rs` — 间隔步进器（受 `INTERVALS` 边界约束，越界时按钮置灰）+ 开机启动开关 + 提醒图片「更换 / 恢复默认」（无自定义图时「恢复默认」置灰，据此区分当前是否已自定义）+ **主题选择区**（「跟随系统」开关 + 浅色/深色两张无文字预览，见下）。间隔与开机启动走 `config::update_settings(&store, |settings| ...)`：加锁、改值、落盘收在这一个函数里；图片更换则把选中的图片复制到 `core::paths::reminder_image_file()`（仅识别得到格式的图片，取消或非图片则不改动），恢复默认即删除该文件。间隔变更随后向调度器发送 `ChangeInterval`。窗口高度随内容增减（现为 540），并给内容区加了 `overflow_y_scroll`（需先 `.id(..)`，`overflow_*` 是 `InteractiveElement` 的方法），以后再加设置项不会把底部裁掉。
  - **失败要当场说**：`save_error` 字段存最近一次失败原因，正文最上方渲染一条提示。失败分两处：`update_settings` 写文件失败，以及 `platform::set_autostart` 写注册表失败 —— 后者会把设置回滚再提示，否则开关显示「已开启」而 Run 键没写进去，下次开机不会启动。主题切换同样：写盘成功才 `set_preference` + `refresh_windows()`。
  - **只有真的成功才动下游**：`set_interval` 落盘成功后才发 `ChangeInterval`，否则本次运行的节奏会和下次启动读到的不一致。
  - `custom_image` 缓存「是否已自定义」，避免每帧 `path.exists()`；两个按钮是它唯一的写入点，写完各自刷新这个字段。

`ui/mod.rs` 是界面共享模块：配色表 `palette`、热力图取色 `calendar_color`、图片格式探测 `image_format`、外观订阅 `follow_system_appearance`、以及自绘标题栏的三个部件 —— `titlebar()`（左侧可拖拽标题 + 右侧按钮槽）、`titlebar_button()`（**标题栏图标按钮的唯一样式来源**：46×38、图标居中、悬停换底色，尺寸与字体来自文件顶部的 `TITLEBAR_*` / `ICON_FONT` 常量）与 `window_button()`（在共用样式之上附加 `WindowControlArea` 的最小化/最大化/关闭语义）。主窗标题栏右侧的**设置齿轮直接复用 `titlebar_button()`**，所以标题栏按钮要改外观只需动这一处；可变的只有四样：`id`、悬停底色、悬停前景色与图标字号（控制键 12、功能键 14）—— 悬停前景色也要传，是因为浅色外观下深色图标落在红色关闭键上会看不清。时间戳换算与文案格式化（`now` / `local_date` / `day_bounds` / `format_clock` / `format_clock_secs` / `format_span` / `format_day_label` / `relative_to_now` / `format_date`）**不在这一层**，而在 `core/time.rs`：`core::data` 也要按本地日切分查询区间，放在 `ui` 会让领域层反向依赖界面。其中 `format_span` 负责把秒数写成「1 天 2 小时 15 分 30 秒」，`format_day_label` 负责把日期转成「昨天 / 前天 / N 天前」。

**主题：跟随系统 / 浅色 / 深色（`palette`）**

配色不再是编译期常量，而是深浅两张表 + 两个 `AtomicU8`：一个记**用户选的主题**（`core::config::Theme`），一个记**系统当前外观**。真正生效的外观由两者合成 —— 用户指定了浅/深就听用户的，否则听系统的（`palette::is_light`）。`palette::current()` 返回对应那张表的引用，所有取色写 `rgb(palette::current().accent)`；改配色只需动这一个模块。

- `core::config::Theme`（`System`/`Light`/`Dark`）落盘成字面量 `theme=system|light|dark`，未知值回退 `System`。**缺省必须是 `System`** —— 旧版本 `settings.txt` 没有这一行，解析错会让升级上来的用户莫名其妙被固定成某一种外观。
- `palette::set_preference(theme)` 记用户选择；`main.rs` 在**创建任何窗口之前**就写入，否则首帧会按默认的「跟随系统」画一帧再被纠正，肉眼可见地闪一下。
- `palette::for_preview(light)` 按「是否浅色」直接取表，**不看用户当前选了什么** —— 只给设置窗的主题预览用，它要在界面上同时画出浅色和深色两张迷你窗口，而此时真正生效的只有一张。
- `ui::follow_system_appearance(window)` 是**每个窗口**都要调一次的订阅入口：先用 `window.appearance()` 校正一次全局值（进程可能长时间没有窗口，期间系统切换深浅就没人更新过配色），再 `observe_window_appearance` 订阅后续变化并 `window.refresh()` 重绘。`Subscription` 是 RAII 守卫，不 `detach()` 会在函数返回时注销掉。
- **必须逐窗订阅**：gpui 的外观观察者是按窗口注册的，Windows 侧来自 `ImmersiveColorSet` 系统广播，每个顶层窗口各收到一份；而配色是全局的，所以三个窗口都订阅，谁先收到都行。
- 颜色只在 GPUI 主线程上被读（每次 `render`）与写（观察者回调 / 设置窗改主题），所以 `AtomicU8` 足够，不需要锁也不存在撕裂。
- 设置窗里选主题：写盘成功后才 `palette::set_preference` + `cx.refresh_windows()`（**三个窗口都要立刻换色**），失败则回滚并走既有的错误提示条。
- 设置窗的主题选择区是**「跟随系统」开关 + 浅色/深色两张无文字预览**。跟随系统是**开关**而非第三张预览（它的含义是「听系统的」，画成一张具体配色反而误导）；开关打开时两张预览**都不选中**（此刻由系统决定用哪张，圈住任何一张都是在撒谎），关掉并手动点了某张，那张才描边点亮。点开关关掉时落到「系统此刻实际是深/浅」那一张。预览卡四角圆角、高 46。
- 设置窗**不可缩放**，所以 `SETTINGS_WIDTH` / `SETTINGS_HEIGHT` 必须自己装得下全部内容（现为 520×540，约 494 的内容 + 余量）。`settings_window_is_tall_enough_for_its_content` 这条测试把各元素高度加起来和 `SETTINGS_HEIGHT` 比，以后加设置项忘了调高度就会失败（clippy 的 `assertions_on_constants` 在这里被 `allow` 掉了，刻意保留）。
- 提醒浮层（`palette::overlay`）**刻意不随外观变化**：整屏遮罩压暗桌面才能让水杯插图与白字立住，浅色外观下换成透明会让插图整个糊进白底。只有「喝了」按钮用界面强调色（浅色下为深蓝，仍是蓝底白字）。
- 热力图 1..=8 级两套外观共用同一段蓝（蓝到足够深时在白底与深底上都立得住），只有 0 次那一档的底色与数字随外观变。

**5. 平台适配层（`platform/`）**

所有 `unsafe` 的 Win32 调用集中于此，且全部提供非 Windows 空实现，保持上层代码零 `cfg` 分支：

- 对外接口一律接收 `&gpui_kit::Window`（只有 `show_main_window` 需要 `&mut`，以便在显示前置脏），原生 HWND 的提取（`HasWindowHandle` → `RawWindowHandle::Win32`）封在内部，调用方不再出现 `raw_window_handle` 依赖与 `cfg` 块
- `style_main_window` / `style_reminder_window` — 两者都经 `set_window_chrome` 设置 DWM 圆角（主窗圆角、浮层直角）。**`style_reminder_window` 还负责把浮层做成真正无边框的全屏**，两件事一起做：
  - **关掉 DWM 非客户区渲染**：`DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED` 去掉四周阴影，`DWMWA_BORDER_COLOR = DWMWA_COLOR_NONE` 去掉那条 1px 描边。不关的话 Windows 给每个顶层窗口画的那圈描边+阴影会留在顶部，看起来像一条白边 —— 「像视频/游戏/浏览器那样的全屏」正是无边框、无阴影。
  - **把客户区对齐显示器矩形**：只信 `display.bounds()` 不够，gpui 把逻辑像素经 `to_device_pixels` 换回物理像素时，`1706.6666 × 1066.6666` 这类值会因浮点截断少几个像素（实测正好 5px），底部就露出一条没被遮罩覆盖的窄带；而浮层带 `WS_EX_TOPMOST`，缺哪条边都是直接看见桌面。`fit_client_to_monitor` 用**实测差值**而非算边框厚度：分别拿客户区在屏幕上的四边与显示器矩形的四边，按差值调整位置与大小，使**客户区**（真正被绘制的那块）恰好等于显示器矩形。不依赖「边框厚度是常数」这个假设 —— 缝隙出现在哪条边、边框变成多厚，都会被拉回 0。全程物理像素，不经逻辑像素换算（5px 正是往返换算截断造成的）
- 浮层开着时用户可能改分辨率、拔接显示器，因此浮层 View 上还挂了 `observe_window_bounds` → `refit_reminder_window` 重新对齐。`fit_client_to_monitor` 是**幂等**的（已对齐时四个差值全为 0 直接返回），所以它自己触发的 `WM_MOVE` 再回调一次也不会递归
- 取不到显示器矩形或客户区矩形时整个跳过，绝不猜一个位置把窗口甩出屏幕
- `enable_system_menu_theme` — 调用 uxtheme 未公开导出 `SetPreferredAppMode`（序号 135）让原生菜单跟随系统暗色主题
- `set_autostart` — 注册表 Run 键的增删，返回 `Result<(), String>`。写入的值是**带引号的完整 exe 路径**：Run 键会被 Windows 直接当命令行解析，装在含空格的目录（如 `C:\Program Files\...`）时不加引号会被按空格切开，开机启动于是静默失效。删除时把 `ERROR_FILE_NOT_FOUND` 当作成功（本来就不存在正是想要的结果）；其它失败一律上报，绝不静默
- `fatal_startup_error` / `warn` — 两个原生 `MessageBoxW`：前者弹完 `process::exit(1)`（返回 `!`），用于「没有它程序就没法用」的情况（托盘建不起来）；后者只是提示后继续跑，用于「这一项没生效但程序照常可用」（自启没写进注册表）。走原生弹框是因为这些故障往往发生在任何 GPUI 窗口存在之前
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
├── core/                    领域逻辑（配置、存储、调度、时间，无 UI 依赖）
│   ├── config.rs            设置模型与 settings.txt 读写、间隔常量表
│   ├── data.rs              SQLite 连接池、按月聚合 + 按需明细、记录读写
│   ├── paths.rs             应用数据目录（%APPDATA%\water-remainder）
│   ├── scheduler.rs         调度线程与 SchedulerCmd / SchedulerEvent 协议
│   └── time.rs              时间戳换算、本地日区间、文案格式化（ui 与 data 共用）
├── ui/                      界面（GPUI View 与共享部件）
│   ├── mod.rs               palette 配色与外观订阅、标题栏与共用按钮样式
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

# 单元测试（纯函数：分档、格式化、本地日区间、设置解析与原子写）
cargo test

# 静态检查（[lints.clippy] 已把 clippy::all 提成 deny，警告即失败）
cargo clippy
cargo fmt --check

# 发布构建（opt-level="z" + fat LTO + strip，体积优先）
cargo build --release
```

发布产物：`target\release\water-remainder.exe`

仓库内提供了打包脚本 `build-copy.ps1`：执行 release 构建后把 exe 覆盖复制到 `D:\executable`，方便直接分发。它走 `cargo build --release --offline`（与上面的联网约定一致），结束时默认暂停等待按键；**被脚本或 CI 调用时加 `-NoPause`**，否则会一直挂住。

## 说明

- 目前仅 Windows 平台做了完整适配（托盘、注册表自启、DWM 样式）；非 Windows 下 `platform.rs` 为空实现，可编译运行但样式与自启能力受限。
- UI 依赖只声明 `gpui-kit` 一项，不再直接依赖 Zed 主仓。`gpui-kit` 是 longbridge 基于 GPUI 的组件库，它内部依赖把 Zed 的 `gpui` 重新发布的 crates.io 快照 `gpui-pre`（本版本对应 `zed@d89e9c2`），并把 gpui 全部根命名空间重导出——`gpui_kit::*` 就是 gpui，`gpui_kit::platform` 就是 `gpui_platform`。
  - 收益：不再需要 git checkout、版本可锁定、`cargo fetch` 走镜像。
  - 代价：底层 rev 由 `gpui-kit` 决定，上游变更只能等它跟进；且 `gpui-kit` 固定为 `gpui_platform` 打开 `font-kit` / `x11` / `wayland` / `runtime_shaders`，为 `gpui-pre` 打开 `windows-manifest`，这些特性无法从本项目侧关闭。
- `default-features = false` 关掉了 gpui-kit 的 `component` 与 `assets`：界面继续完全由本项目自绘（`palette` + `ui.rs` 的标题栏部件），不引入 shadcn 主题，也不嵌入图标资源。
- 启动时调用一次 `gpui_kit::init(cx)`（gpui-kit 的契约）。当前只启用 gpui 层，它实际只登记了 `gpui-base` 的主题与各控件全局态，本项目界面自绘、不读这些全局量。
