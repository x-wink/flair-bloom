# 架构

气质花（FlairBloom）是面向游戏辅助的按键助手：Tauri v2 单进程，Rust 后端跑全局键鼠钩子与注入，React 前端做面板与浮窗。核心功能免费，亲友专属功能通过 Ed25519 离线兑换码激活。

## 应用模式

| 模式                       | 现状                                                                                                                                                  |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| 面板（`panel.html`）       | 全功能配置界面。一切最小化路径都收进浮窗——标题栏按钮、面板显隐热键，以及系统级最小化（Win+D / 任务栏 / Aero Shake，由 `on_window_event` 的 Resized + `is_minimized` 接管，先 `unminimize` 再进浮窗模式）；关闭按钮与 Alt+F4 都走 `handleClose`，按 `closeBehavior` 偏好收进浮窗或退出，未设偏好时每次询问（默认） |
| 浮窗（`panel-float.html`） | 常驻置顶胶囊，显示激活规则、全局开关、展开主面板。Alt+F4 关浮窗 = 退出应用（`CloseRequested` 被 `prevent_close` 后转 `shutdown_and_exit`）：浮窗只有一行胶囊大小，放不下确认对话框，而放任它销毁会让收起状态的应用只剩托盘，且窗口销毁后无法再 `show` 回来 |
| 托盘                       | 面板隐藏后进程常驻，连发继续有效。托盘菜单：全局开关、切换配置（动态菜单项）、打开面板、退出；托盘双击与再次启动应用都唤回面板；启用 / 禁用态切换图标。托盘是辅助入口而不是唯一入口：浮窗不占任务栏位，若面板也缩进任务栏，应用就只剩一个会被折叠起来的托盘图标 |
| 桌宠（`pet.html`）         | 未实现，设计见 `docs/roadmaps/pet-mode.md`（draft）                                                                                                   |

## 进程模型

```
FlairBloom.exe（单一 Tauri 进程）
  Rust 后端：连发引擎 / 系统托盘 / 更新检查 / 驱动诊断 / 许可证校验
      │ Tauri events + invoke
      ├── 面板 WebView（panel.html）
      └── 浮窗 WebView（panel-float.html）
           │ HTTPS
   GitHub Releases ← gh-proxy.com 加速（直连兜底）
```

**两态不变量**：运行时面板与浮窗恰有一个可见，不存在「两个都藏起来只剩托盘」的中间态——托盘会被用户折叠，那时应用等于失踪。守住它的是三处：系统级最小化被 `on_window_event` 接管进浮窗模式、浮窗的关闭请求转为退出、`enter_float_mode` 在浮窗缺失时保留面板。

**退出只有一个出口**：`lib.rs` 的 `shutdown_and_exit`（先 `engine.shutdown()` 再 `app.exit(0)`），托盘「退出」、`exit_app` 命令（面板关闭选「直接退出」、协议对话框「不同意并退出」）、浮窗关闭请求都走它。不走 `window.destroy()`：销毁面板时浮窗窗口仍在窗口表里，进程不会退出，引擎也不会停，结果是窗口全没了但连发还在后台跑。`RunEvent::Exit` 里再 `shutdown()` 一次，兜住不经此函数的退出路径。

**单进程多窗口**：面板与浮窗是同一进程的独立 WebView，通过 `app.emit_all()` 事件通信（`float-active` / `global-enabled-changed` / `theme-changed` / `app-status-changed` / `update-*`），无 Named Pipe。窗口显隐统一走 `lib.rs` 的 `enter_panel_mode`（显示面板、隐藏浮窗）/ `enter_float_mode`（先显示浮窗再隐藏面板，浮窗缺失则保留面板）。激活态规则当前由前端轮询 `get_active_rules`。

WebView 聚焦时 `WH_KEYBOARD_LL` 全局钩子不触发，面板与浮窗都用 `useKeyRelay` 把键盘事件中继到后端 `relay_key_event` 命令，交由引擎统一处理热键 / Toggle 触发 / `pressed_keys` 维护，避免聚焦窗口时热键被吞；同时阻止非编辑区的默认快捷键（F12、Ctrl+Shift+I 等，仅生产构建）。

**AppHandle 不进 packages**：`win-driver` / `win-input` / `win-sysinfo` / `burst-engine` 所有函数均不接受 `AppHandle`。资源目录由 `commands/driver.rs` 从 `app.path().resource_dir()` 取得后传入，Tauri 状态管理留在 commands 层。

## Crate 依赖图

```
packages/crypto      packages/migrate
         ↑                ↑   ↑
         └────────────────┘   │
    packages/qzh-format       │
         ↑                    │
    packages/qzh-profile ─────┘
         ↑
    apps/main/src-tauri      apps/keygen（依赖 crypto）

packages/win-input  ← packages/burst-engine ← apps/main/src-tauri
packages/win-driver / win-sysinfo / resource-integrity ← apps/main/src-tauri

apps/site（独立 pnpm 项目，消费私有制品 @xwink/ui；引用 apps/main 的 theme.ts 与 markdown-parse.ts）
```

## 输入模式与注入通道

`win_input::dispatch(KeyId, is_up)` 是统一入口，按 `(mode, KeyId)` 分发到对应后端。三档通道由用户在设置中选择：

| 模式                           | 实现                                                    | 说明                                                                                                                                                                                                                                         |
| ------------------------------ | ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SendInput（通用模式，默认）    | `win-input/src/lib.rs`                                  | 键盘 `SendInput INPUT_KEYBOARD` + `KEYEVENTF_SCANCODE`；鼠标 `INPUT_MOUSE` + `MOUSEEVENTF_*`（X1/X2 用 `MOUSEEVENTF_XDOWN/UP` + `mouseData=XBUTTON1/2`）。`dwExtraInfo = SIM_MARKER` 标记自身注入事件防循环                                  |
| Interception（游戏模式，主推） | `win-input/src/interception.rs`                         | 键盘 + 鼠标设备各扫描一次，鼠标状态位映射 `INTERCEPTION_MOUSE_BUTTON_4/5_DOWN/UP`（X1/X2）。`interception_send` 返回写入 stroke 数，鼠标 / 滚轮失败回退 SendInput。需要安装驱动并重启，以管理员运行                                          |
| DDSimple（DD驱动，备用）       | `win-input/src/ddsimple.rs` + `dd_common.rs`（dd63330） | 键盘 `DD_key`，鼠标走 `MOUSE_INPUT_DATA.ButtonFlags`，原生支持 X1/X2 侧键；滚轮用 DD SDK 上滚 / 下滚编码。无需重启但需管理员。`ExtraInformation` 被驱动写死为 0，`SIM_MARKER` 无法幸存，自注入回灌改由 `PENDING_INJECTIONS` 时间窗口队列过滤 |

X1/X2 在 DD 模式 / 鼠标设备缺失时按 once 旗标 warn 一次后自动回退 SendInput。`dd63330.dll` 纳入打包资源、运行时完整性校验（`packages/resource-integrity`）与发版前 `pnpm check:resources`。

**DD-HID 已永久移除**（驱动不稳定会导致蓝屏）：注入后端、`InputMode::DdHid`、安装链路与 `ddhid.63340.dll` 全部删除，`InputMode::from_str("dd_hid")` 返回 `None`，存量用户的 `input_mode` 配置在 `collect_configured_input_mode` 自动回落 SendInput。仅保留卸载与残留清理：`win_driver::dd_hid` 的检测与 `uninstall`、`uninstall_dd_hid_driver` 命令、`repair_dd_hid_residue`、诊断报告导出，以及随包分发的 `ddhid-driver/`（`ddc.exe -u` 要用）。

**输入模式与布局互斥**：DDSimple 的单键规则约束无法用横版键鼠图表达（横版每个键位都是 `trigger == target` 的重合态，DD 下重合态切换连发是空集，见 [key-policy](key-policy.md)），故二者互斥——`selectInputMode`（横版下禁选 DD 驱动）与 `switchLayout`（DD 驱动下禁切横版）两个 choke point 拦截并提示。后者按后端下发的 `coincident_toggle` 能力位判断而非模式名，且用 `aria-disabled` 而非 `disabled`：禁用的按钮不触发 `onClick`，原因提示就永远跑不到。

## 连发引擎（`packages/burst-engine`）

`windows_sys` `WH_KEYBOARD_LL` + `WH_MOUSE_LL` 双低级钩子共用同一消息循环线程，监听键盘与鼠标 5 键（左 / 右 / 中 / X1 / X2，含 `WM_XBUTTONDOWN/UP` 高 16 位识别 X1/X2）及滚轮（`WM_MOUSEWHEEL`，每格瞬发 press+release）。

- 注入事件先在 hook 层过滤：SendInput / Interception 用 `SIM_MARKER`，DDSimple 用 `PENDING_INJECTIONS`（以 `(KeyId, is_up)` 为键）。
- 引擎用 `pressed_keys: HashSet<KeyId>` 记录已按下的物理键，只让首次 down 进入 `on_key_press`，up 时移除；不依赖 `KBDLLHOOKSTRUCT.flags` 的保留位判断 key-repeat。
- 线程编排：`catch_unwind` 包裹引擎线程，panic 后记录日志并补发释放事件；并发连发用 `AtomicBool cancel + thread::park_timeout`，`Drop` 时先 signal 再 join 确保按键不卡住。规则热更新前停止连发线程并清空 toggle 状态。
- Toggle 互斥分组：同组激活一条自动停止同组其他活跃规则；同组切换时只播报「新规则开始」。
- 多规则隔离：模拟目标键不会触发其他规则的启动 / 停止逻辑。
- 非 Windows 平台提供空实现（`cfg(windows)` 隔离）。

**总并发限速**：连发间隔结构下限 1ms（旧配置兼容），有效下限 `MIN_EFFECTIVE_INTERVAL_MS = 10ms`——全局 LL 钩子链 + RIT 对每个注入事件有固定「过路税」，管线可持续的总注入速率有上限。加载时 `Profile::clamp_intervals()` 把 <10ms 静默钳到 10ms（所有读取入口经 `load_from_path` / `read_profile_from_file` 这一处）。运行时 `process_due` 按「基础下限 × 当前活跃规则数」放大每条规则的有效下限，使总 tap 速率 ≈ 1000/基础下限、与规则数无关，避免多规则叠加超发导致停止后「收不住」。只拉长拍间间隔、不改 `hold` 点按时长。

## 全局热键

`global_toggle` / `global_stop` / `panel_toggle` 不走 `tauri-plugin-global-shortcut`，与连发规则共用 `burst-engine` 低级 hook：热键检测优先于规则处理，且不受 `global_enabled` 当前状态限制，启动期立即生效。只允许绑定键盘实体键、三者互不重复——在绑定 UI 拦截（`KeyCapture` 按槽位允许集收键 + `SettingsDialog` 去重校验），避免同一键被某个热键抢先处理导致其余功能失效。热键可绑修饰键（左右 Shift / Ctrl / Alt / Win），连发规则不可。

AltGr 布局（德语 / 法语 / 波兰语等）把右 Alt 当 AltGr，键盘驱动会在它前面补发一个左 Ctrl，低级钩子看到两个独立事件，因此右 Alt 与左 Ctrl 同时绑热键会互相牵连——不做代码过滤（合成 Ctrl 与物理 Ctrl 在钩子层不可靠区分），改由 `SettingsDialog` 在绑定右 Alt 后按是否也绑了左 Ctrl 分两级提示。

## 前端结构要点

- 面板根组件 `PanelApp.tsx`：竖版规则列表 + 横版键鼠图（`HorizontalLayout.tsx`，键位表在 `keyboardLayout.ts`），布局各自定窗口尺寸（竖版 405×720、横版 1060×580），切换时 `setSize + center`。
- 设计 Token 在 `theme.css`，主题预设色板与亮 / 暗 / 跟随系统在 `theme.ts`（写 `data-theme` 到根元素）。所有组件通过变量取色取尺寸。
- `components/Overlay.tsx` 提供锚定定位（12 方位 + 视口钳位）与遮罩；Toast / ConfirmDialog / ContextMenu 都建在它上面。层级 Token：`--fb-z-overlay-*` 供弹窗，`--fb-z-tour-*` 供新手引导（要叠在设置弹窗上）。
- 新手引导 `tour/`：`TourRunner.tsx` 是唯一有 DOM 逻辑的文件（四块遮罩镂空、锚定气泡、实操判定），教程内容在 `tour/tours/<id>.ts`，只经 `TourHost` 接口操作宿主；进度存 `settings.json` 的 `tours` 键：`completed` 是学过的教程，`introVersion` 是已自动展示过的首启教程版本——与用户协议同一套语义，`TOUR_INTRO_VERSION`（`tour/types.ts`）一 bump，老用户下次启动会再自动看一遍。判定刻意不看「有没有规则」：新装配置自带两条未启用的出厂规则，那个口径对新用户永远为假。说明书大纲在本 skill 的 `references/manual/<id>.md`，`pnpm skills:check` 钉住一一对应。
- 冲突检测 `conflicts.ts` 管跨绑定的软性提醒（面板键遮蔽全局键、热键盖住规则触发键、A 的连发键是 B 的触发键），与后端的硬性录入策略互不替代。

## 数据存储路径

| 数据               | 路径                                                                   | 说明                                                       |
| ------------------ | ---------------------------------------------------------------------- | ---------------------------------------------------------- |
| 配置文件（`.qzh`） | `{app_data_dir}/profiles/`                                             | 导入 / 导出经文件对话框，不暴露内部路径                    |
| 自选提示音         | `{app_data_dir}/sounds/`                                               | 导入时复制进来，settings.json 只存文件名                   |
| 应用设置           | `{app_data_dir}/settings.json`                                         | `tauri-plugin-store`；复用 `packages/migrate` 迁移基础设施 |
| 待安装更新包       | `{app_local_data_dir}/pending_update/`                                 | 静默下载完成后暂存                                         |
| 日志               | `{app_log_dir}/`（Windows `%LOCALAPPDATA%\fun.xwink.flairbloom\logs`） | 按天滚动，保留 7 天，`cleanup_old_logs` 清理               |
| 崩溃日志           | `{app_log_dir}/crash-{unix_ts}.log`                                    | panic hook 写入                                            |

路径由 Tauri `PathResolver` 跨平台解析；日志目录由 `bootstrap/logging.rs` 显式定义。卸载时用户配置、设置与日志保留（Windows 惯例），注册表自启动项与「以管理员模式启动」的兼容性标志（`HKCU\...\AppCompatFlags\Layers`，值名是 exe 路径）由安装器清除。

## 日志与崩溃

- Rust 端 `tracing` + `tracing-subscriber` + `tracing-appender`，结构化格式（时间戳、级别、模块、线程 ID），默认 INFO。分级：ERROR 功能不可用（引擎崩溃、文件损坏）；WARN 降级运行（重试成功、配置回退）；INFO 关键状态变更（连发启停、配置切换、教程开始 / 完成）；DEBUG 每次按键事件、IPC 消息，默认关闭。
- 前端 JS 错误经 `log_from_frontend` 命令转发到同一日志文件。
- `std::panic::set_hook` 写独立崩溃日志并弹 rfd 原生对话框（独立线程展示、进程级去重）：panic 可能发生在 LL hook 等敏感线程，原地阻塞会拖垮系统输入；正文直接展示崩溃日志完整路径，不单设「复制路径」按钮。
- 关于 / 诊断入口可打开日志、数据、安装、驱动目录（`open_app_dir` 白名单）。日志不含硬件 ID、用户名。

## 容错策略

| 场景                         | 策略                                          |
| ---------------------------- | --------------------------------------------- |
| 连发循环线程 panic           | `catch_unwind` 捕获，记录日志，补发 `key_up`  |
| 配置文件损坏 / 篡改          | 提示用户，提供恢复默认配置，不阻塞启动        |
| 自动更新下载失败             | 前端提示，不影响使用                          |
| 全局监听或输入后端初始化失败 | 提示切换模式 / 以管理员运行，其余功能保持可用 |
| 规则热键冲突                 | 界面标注并提示                                |
| Tauri Command 异常           | 所有 Command 返回 `Result`，前端统一 toast    |

## 技术选型

| 用途                               | 库 / 工具                                                                       |
| ---------------------------------- | ------------------------------------------------------------------------------- |
| 全局键鼠监听                       | `windows_sys` `WH_KEYBOARD_LL` / `WH_MOUSE_LL`                                  |
| 按键 / 鼠标模拟                    | `windows_sys` `SendInput`；Interception；DD SDK（dd63330）                      |
| 自动更新                           | `tauri-plugin-updater`                                                          |
| 配置加密 / 密钥派生 / 许可证       | `aes-gcm`、`hkdf` + `sha2`、`ed25519-dalek`、`base32`                           |
| 应用状态持久化 / 开机自启 / 单实例 | `tauri-plugin-store`、`tauri-plugin-autostart`、`tauri-plugin-single-instance`  |
| 前端                               | React + TypeScript + 原生 CSS，Vite 多入口（`panel.html` / `panel-float.html`） |
| Monorepo                           | Cargo workspace + pnpm workspaces                                               |

## 已确认的风险与对策

| 风险                                        | 对策                                                   |
| ------------------------------------------- | ------------------------------------------------------ |
| 低级 hook 与模拟输入自循环                  | `SIM_MARKER` + `PENDING_INJECTIONS` 过滤               |
| 反作弊拦截模拟输入                          | 不做技术规避，EULA 与文档明确说明                      |
| AES 密钥被逆向提取                          | 接受「防普通用户」定位，不对抗专业逆向                 |
| 系统时间回拨绕过许可证                      | payload 含 `issue_time` 做下界校验                     |
| 更新包被中间人替换                          | Tauri updater 强制 `.sig` 签名验证 + HTTPS             |
| 驱动残留                                    | 诊断修复提供安装前置检查、残留识别、深度清理与重启提示 |
| WebView 聚焦吞掉热键                        | 前端中继键盘事件到后端                                 |
| 坏版本经静默更新即时铺开且 updater 不可降级 | 见 [release](release.md) 的护栏与应急回退              |
