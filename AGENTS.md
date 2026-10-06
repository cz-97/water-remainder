# AGENTS.md

本文件是仓库内的 Agent 工作约定。README 负责面向用户的功能介绍；这里保留验证流程、实现边界和不容易从代码直接看出的约定。涉及具体实现时，应先读当前源码并以源码为准，避免把文档中的旧描述当作事实。

## 改动后的固定流程

每次修改源码后按顺序完成以下步骤。测试按改动范围选择，不要求每次都跑完整套件。文档专属改动不需要运行 Rust 构建或 GUI；除用户明确要求外，不要仅因本文件写有提交流程就替用户提交。

1. **编译与静态检查**
   - 运行 `cargo check --offline`。失败时先查明并修复原因，通过后再继续。
   - 只有改动了有边界风险或回归代价较高的纯逻辑时，才运行相关测试：优先用 `cargo test --offline <测试名片段>` 跑对应测试；涉及多个模块或共享逻辑时再运行 `cargo test --offline`。典型范围包括提醒间隔归档、日期区间、配置解析/持久化及失败回滚。单纯界面布局、颜色、文案或样式调整不要求跑测试。
   - 运行 `cargo clippy --offline` 和 `cargo fmt --check`。`Cargo.toml` 将 `clippy::all` 设为 `deny`，新增 lint 会使命令失败。
   - `rust-toolchain.toml` 使用 `stable` 通道并声明 rustfmt/clippy。不要将通道改成精确版本号来处理本机缺少工具链的问题，以免 rustup 尝试联网下载。
2. **运行验证**
   - 启动前先结束本程序已有实例，再用 `cargo run` 构建并启动；不要直接 `Start-Process` 启动 `target` 中可能过期的 exe。`cargo check`、Clippy 和测试都不保证生成最新可运行 exe。
   - 本程序使用不区分路径的单实例互斥体。同名程序可能从不同目录运行；若有多个副本，先查看可执行文件路径并确认实际实例已退出，不能只凭进程名或 `cargo run` 的输出断定新实例已启动。
   - 程序启动后保持托盘常驻，不要为了结束验证而退出它。依赖未缓存且网络不可用时，运行 `cargo run --offline` 并如实报告无法满足的验证。
   - 验证 DPI、多显示器、托盘、注册表、电源事件等系统行为时，使用可信系统数据或真实交互确认。截图探测前确认坐标/DPI 语义；无法可靠观察或必须等待特定时机时，请用户协助，不要反复轮询或临时改用户设置制造条件。
   - 需要展示提醒浮层时使用托盘「立即提醒」入口。不要为了触发提醒而改动提醒间隔。
   - `%APPDATA%` 下的配置、记录及注册表自启动项是用户真实状态。测试不得写入假记录；确实需要临时更改时，完成后恢复原值。
   - 纯逻辑测试应使用隔离的临时数据，不触碰真实用户目录。纯界面调整可用编译/格式检查和启动后的实际界面观察验证，无需为了流程制造对应的单元测试。
   - 若本次只改测试、注释或文档且不影响运行代码，可跳过启动 GUI；改动可见界面或系统行为时再运行并观察。
3. **提交**
   - 只在用户要求提交或当前任务明确要求提交时执行。提交前确认检查已通过，只暂存本次改动，不包含构建产物或用户已有的其它修改。
   - 按下方版本规则判断是否需要同时修改 `Cargo.toml` 的版本号，并将该改动一并纳入提交。
   - 提交首行不超过 50 字符，使用祈使语气、首字母大写且不加标点。需要正文时空一行，正文每行不超过 72 字符，解释原因且不重复标题。

命令的非零退出码、异常终止或空输出都要查证，不能直接归因于沙箱或环境后跳过。验证动作应能区分本次改动是否生效；通用规则不要写死某次部署目录或某次故障的具体路径。

## 版本号规则

当前包版本见 `Cargo.toml`，格式为 `MAJOR.MINOR.PATCH`。版本号传达变更类型，不代表工作量、视觉差异大小或修复影响范围。SemVer 对稳定公开 API 的基本定义是：兼容缺陷修复递增 PATCH，向后兼容的新功能递增 MINOR，不兼容变更递增 MAJOR；`0.y.z` 表示初始开发阶段，不承诺 API 稳定。Cargo 对 `0.x` 的依赖解析也把左侧非零段作为兼容边界。

本项目是桌面应用而非供外部 Rust crate 调用的稳定库，因此按用户可感知的**能力与兼容性**执行以下发布约定：

- **PATCH：修正已有行为。** 让现有功能符合预期的缺陷修复，以及不改变能力范围的稳定性、布局、文案、配色、间距、DPI/边缘缝隙等修正，均递增 PATCH。界面看得见的变化本身不构成 MINOR 理由。
- **MINOR：新增或显著扩展用户能力。** 例如增加一种设置、独立视图、交互入口，或让用户能完成此前无法完成的一类操作。一个版本内多个新增项只递增一次。单纯调整已有控件外观或修复其行为仍是 PATCH。
- **不兼容变更：** 对 `0.x` 项目按 Cargo/SemVer 的初始开发约定递增 MINOR；例如无法读取旧版用户数据、破坏设置迁移、移除现有能力或改变用户依赖的持久化契约。到 `1.0.0` 后此类变更递增 MAJOR。
- **不升版本：** 纯内部重构、注释/文档、构建脚本、`.gitignore`，以及不改变用户能力或兼容性的依赖维护。
- 一次提交最多递增一个段；递增 MINOR 时 PATCH 归零，递增 MAJOR 时 MINOR 与 PATCH 归零。不要把小型界面调整包装成新功能来抬高版本。

参考： [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html)、[Cargo SemVer Compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)。

## 技术栈与依赖

- **语言/工具链：** Rust 2024；工具链通道见 `rust-toolchain.toml`。
- **窗口与绘制：** `gpui-kit`，关闭默认 features，仅使用 GPUI/平台层；窗口和视图实现位于 `src/ui/`。
- **托盘和菜单：** `tray-icon` 提供托盘入口；`gpui-kit` 所依赖的 `muda` 菜单事件与应用事件在入口汇聚。
- **Windows 系统集成：** Windows 目标依赖 `windows` crate，处理 Win32、DWM、注册表、互斥体和文件选择对话框；`raw-window-handle` 用于从 GPUI 窗口取得原生句柄。
- **时间：** `chrono` 提供本地日期、时间戳和日界限转换。
- **记录存储：** `rusqlite` 使用 bundled SQLite；`r2d2` 与 `r2d2_sqlite` 提供连接池。
- **配置序列化：** `serde` 派生配置类型的序列化/反序列化，`toml` 读写 `%APPDATA%` 下的 TOML 设置文件；语法无效时启动提示错误，缺失或类型错误字段使用默认值。
- **事件通道：** `std::sync::mpsc` 用于调度线程，`futures-channel` / `futures-util` 用于转发到 GPUI 事件循环。
- **资源嵌入：** `build.rs` 使用 `embed-resource` 编译根目录的 `water-remainder.rc`；资源图位于 `src/assets/`。

修改依赖时先确认它属于哪个目标/功能、是否已由现有依赖提供；不要因为间接依赖可用就假设它是本项目可直接使用的 API。`Cargo.lock` 应与依赖解析变更保持一致。

## 源码职责

- `src/main.rs`：应用装配、单实例检查、GPUI 启动、托盘/菜单/调度事件汇聚，以及系统唤醒时通知调度器。
- `src/core/config.rs`：设置模型、间隔选项、设置文件解析与原子写入、设置更新与失败回滚。
- `src/core/data.rs`：SQLite 连接池、喝水记录读写、按月计数/按日明细缓存及最早/最近记录边界。
- `src/core/time.rs`：系统时间、本地日期和日区间，以及时间/日期文案格式化；供领域逻辑与界面共同使用。
- `src/core/scheduler.rs`：独立调度线程、deadline 计算与 `SchedulerCmd` / `SchedulerEvent` 协议。
- `src/core/paths.rs`：应用数据目录及配置、数据库、提醒图片路径。
- `src/ui/main_window.rs`：记录月历与所选日期时间轴。
- `src/ui/settings_window.rs`：提醒间隔、自启动、主题和提醒图片设置。
- `src/ui/reminder_window.rs`：全屏提醒浮层、倒计时和喝水/跳过交互。
- `src/ui/mod.rs`：共享配色、系统外观订阅、标题栏与按钮样式等 UI 支持代码。
- `src/platform/mod.rs`：平台接口和 Windows 原生窗口、DWM、注册表、单实例、消息框等适配；上层不应自行提取 HWND 或散布平台条件编译。
- `src/platform/tray.rs`：托盘图标与菜单建立。
- `build.rs`、`water-remainder.rc`：Windows 资源编译配置；`release.ps1`：构建、部署与发布脚本。

## 长期实现约定

- **分层：** 可独立表达的配置、时间、分档等逻辑放在 `core/`，不依赖 UI；窗口表现放在 `ui/`；Win32 `unsafe` 及系统状态访问集中在 `platform/`。新增模块遵循实际职责边界。
- **错误处理：** 用户能感知且会影响后续行为的持久化失败必须反馈；若先更新了内存状态，失败时回滚。启动关键故障应可见，非致命功能失败应明确降级，避免静默假成功。
- **存储与绘制：** SQLite 是记录持久来源；沿用按需查询和缓存边界，避免把数据库读操作放进每帧渲染路径。外部直接改库的缓存影响按现有实现处理，不引入未经设计的失效策略。
- **生命周期：** GPUI 窗口订阅和后台任务需确认其生命周期与事件循环相匹配。RAII 订阅若需要跨启动闭包存活，应由正确的持有者保存或显式 detach。
- **非 Windows 构建：** 平台接口保留对应的非 Windows 实现，使领域/UI 层不必散布 `cfg`；这不表示非 Windows 的托盘、注册表或窗口样式行为已与 Windows 等价。

## 用户数据位置

- `%APPDATA%\\water-remainder\\settings.toml`：设置和主窗口状态。
- `%APPDATA%\\water-remainder\\data.db`：喝水记录 SQLite 数据库（运行时也可能有 WAL/SHM 文件）。
- `%APPDATA%\\water-remainder\\reminder.img`：自定义提醒图片；不存在时使用内置图片。
- Windows 自启动：`HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run` 下的应用项。

调试时不要把上述路径或某台机器的部署目录写进源码规则；路径应由代码中的路径模块和当前系统查询确认。

## 部署与发布

用户的用词有固定含义，不要自行替换或合并。两种模式由 `release.ps1` 的参数决定：

| 用户说 | 含义 | 命令 |
| --- | --- | --- |
| **「部署」** | 只做本地部署 | `.\release.ps1` |
| **「发布」** | 部署 **加上传发行版** | `.\release.ps1 <版本号>`，如 `.\release.ps1 0.11.0` |

- **「部署」不推 tag、不建发行版。** 不传版本号即为部署。
- **「发布」是两步，缺一不可**：本地部署 + 上传发行版。脚本按顺序做完：
  结束运行中实例 → release 构建 → 复制到部署目录 → 改 `Cargo.toml` 版本号 →
  提交 → 推 master → 建 tag → 推 tag。缺任一步都算没做完。
- **版本号由脚本写入 `Cargo.toml`**，不要手改。手改容易漏，而漏了要等一轮 CI
  才在版本核对那步失败。
- 脚本会在动手前检查：工作区必须干净、当前分支必须是 master、版本号必须是三段数字
  （`v0.11.0`、`0.11.0-beta` 都会被拒）。这些是有意卡住用户的，不是缺陷。
- **tag 已存在时脚本会拒绝**，不覆盖。覆盖会让 GitHub 上已发布的版本与附件错位；
  确需重新发布时先手工 `git tag -d` 并 `git push origin :refs/tags/<tag>`。
- **推送成功不等于发布成功。** Actions 仍可能失败，必须等 run 结束并确认 release
  上出现了附件：
  ```
  gh run watch --repo cz-97/water-remainder
  gh release view <tag> --repo cz-97/water-remainder
  ```

### 推 tag 即上传发行版

`git push origin vX.Y.Z` 触发 `.github/workflows/release.yml`：在 windows-latest 上构建，
创建 GitHub Release 并附上 `water-remainder-<version>-windows-x64.exe` 与 `.sha256`。

- **tag 必须与 `Cargo.toml` 的 `version` 一致**，否则 workflow 第一步就失败。
- **GitHub 读的是 tag 指向的那个 commit 里的 workflow 文件**，不是 master 的。
  改了 workflow 又想让新步骤生效，必须把 tag 移到含新 workflow 的 commit 上再推。
- 发行版面向用户，**改动可见行为或系统行为时更新 `CHANGELOG.md`**，正文用中文。
- workflow 跑在远端，本机看不到它的日志。推送失败要看 `gh run view --log-failed`，
  不要因为「命令返回 0」就认为发布成功。
- **版本号不需要连续。** `Cargo.toml` 的 `version` 在发布时才推进，日常提交不动它；
  攒够够格的改动发一次即可，线上的号自然会跳号（0.4.1 → 0.10.0）。这与
  「按改动程度决定升哪一段」的版本规则不冲突。
