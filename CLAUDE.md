# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**气质花（FlairBloom）** — 面向游戏辅助的按键助手。核心功能免费，亲友专属功能通过 Ed25519 离线兑换码激活。

详细规划见 `docs/ROADMAP.md`，资源清单见 `docs/ASSETS.md`。  
`README.md` 即面向用户的使用说明书，`apps/main/src/assets/EULA.md` 为用户协议，`THIRD_PARTY.md` 为第三方组件声明。

## 常用命令

```sh
pnpm dev                        # 启动 Tauri 开发模式（热重载）
pnpm build                      # 构建生产包
pnpm lint                       # oxlint 检查前端代码
pnpm lint:fix                   # oxlint 自动修复
pnpm format                     # oxfmt 格式化前端代码
pnpm format:check               # oxfmt 格式检查（CI 用）

cargo check                     # 检查所有 workspace crate
cargo fmt                       # 格式化所有 Rust 代码
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p <crate>           # 运行指定 crate 的测试，如 -p crypto

pnpm coverage                   # 共享 crate 覆盖率（CI 同源），低于阈值会红灯
pnpm coverage:html              # 浏览器打开 HTML 报告，定位未覆盖行
```

克隆后执行一次：

```sh
git config core.hooksPath .githooks
```

## Monorepo 结构

```
apps/main/src-tauri/src/        # Tauri 后端（Rust）
  lib.rs / main.rs              # 应用入口（~100 行薄壳）、窗口创建、事件注册
  tray.rs                       # 系统托盘
  bootstrap/                    # 启动期装配（不含 Tauri 命令）
    logging.rs                  # tracing 初始化 + panic hook + 旧日志清理
    agreement.rs                # check_agreement + AGREEMENT_VERSION
    update.rs                   # UpdateLock + 静默更新 + check_for_updates
    profile.rs                  # load_or_init_profile（委托 qzh-profile）
    input.rs                    # init_input_backend + parse_switch_mode_arg
  commands/                     # 前端 invoke 入口（纯 Tauri 桥接）
    app.rs                      # agree_license / check_update / exit_app
    driver.rs                   # 驱动安装卸载 + is_elevated + relaunch_as_admin
    engine.rs                   # 规则 CRUD + 输入模式切换（~150 行）
    log.rs / profile.rs         # 日志 / 配置文件 CRUD
    repair.rs                   # diagnose_environment + 4 个 repair_* 命令
    status.rs                   # get_app_status + emit_status_changed
  engine/
    mod.rs                      # 仅 re-export burst_engine / win_input 公开 API
apps/main/src/windows/panel/    # 面板窗口（React）
  main.tsx                      # 入口，挂载 Provider
  PanelApp.tsx / .css           # 根组件
  theme.css                     # 设计 Token（颜色/间距/字号变量）
  components/                   # UI 基础组件
  dialogs/                      # 弹窗内容组件
apps/main/src/assets/icons/     # SVG 图标源文件（currentColor / 1em 尺寸）
apps/keygen/                    # 兑换码生成 CLI
packages/crypto/src/
  aes.rs                        # AES-256-GCM encrypt/decrypt
  license.rs                    # Ed25519 verify_license + LicensePayload
packages/migrate/src/lib.rs     # run_migrations() 泛型迁移运行器
packages/qzh-format/src/
  header.rs                     # FileHeader（Magic/Version/Flags/Nonce）
  lib.rs                        # read_encrypted / write_encrypted 高层 helper
packages/qzh-profile/src/
  key_id.rs                     # KeyId（Keyboard(VK) | Mouse(MouseButton)）+ MouseButton 5 键
  profile.rs                    # Profile / BurstRule 数据结构 + validate()
  macro_seq.rs                  # MacroSequence / MacroStep + MAX_STEPS=256（亲友功能）
  schema_migrate.rs             # migrate_profile()，调用 packages/migrate
  lib.rs                        # load_from_path / save_to_path 高层 helper
packages/win-sysinfo/src/
  lib.rs                        # os_version / webview2_version / host_arch / user_locale / install_path
  registry.rs                   # wide / RegRoot / read_reg_* / service_key_present / is_interception_service
  prereq.rs                     # detect_hvci / detect_sac / detect_pending_reboot / Defender 排除路径
packages/win-input/src/
  lib.rs                        # InputMode / init_backend / dispatch / SIM_MARKER / PENDING_INJECTIONS
  ddhid.rs / dd_common.rs       # DD-HID 后端
  interception.rs               # Interception 后端 + is_driver_installed()
packages/burst-engine/src/
  lib.rs                        # BurstEngine + start_listener（LL keyboard/mouse hook + 消息循环）
packages/win-driver/src/
  elevation.rs                  # is_process_elevated / run_elevated_exe / run_elevated_exe_capture
  powershell.rs                 # run_script_elevated / ps_single_quoted / ps_string_array / base64_std_encode
  dd_hid.rs                     # dd_hid_sys_path/installed / install / uninstall / find_dd_hid_oem_inf
  interception.rs               # install / uninstall（调用 install-interception.exe）
  judge.rs                      # judge_install_result / judge_uninstall_result
  path_util.rs                  # strip_verbatim（去掉 verbatim 路径前缀）
```

## 关键架构决策

**单进程多窗口**：面板（`panel.html`）和桌宠（`pet.html`，v0.3 加入）是同一 Tauri 进程的独立 WebView，通过 `app.emit_all()` 通信，无 Named Pipe。

**配置文件格式（.qzh）**：`FileHeader`（19 字节，含 Nonce）+ AES-256-GCM 密文 + Auth Tag。Header 的 `magic+version+flags` 作为 AAD 防篡改。JSON payload 首字段 `schema_version` 驱动 `qzh-profile/src/schema_migrate.rs` 迁移链（Strategy B）。当前 `CURRENT_SCHEMA_VERSION = 2`（v1→v2：所有按键字段从裸 `u32` VK 升级为 [`KeyId`]，向后兼容自动迁移）。`tauri-plugin-store` 的 settings.json 复用同一迁移基础设施（`packages/migrate`）。

文件读写高层入口：`qzh_format::read_encrypted(path)` / `qzh_format::write_encrypted(path, &T)` 封装了 header+aad+decrypt+parse / serialize+encrypt+atomic-rename 五连段；`qzh_profile::load_from_path(path)` / `qzh_profile::save_to_path(path, &profile)` 在此基础上再叠加 schema 迁移与业务校验。

**按键标识 [`KeyId`]**：tagged union，前后端共享 wire format `{kind:"keyboard",code:81}` / `{kind:"mouse",code:"left"}`。`MouseButton` 含 `Left/Right/Middle/X1/X2`。所有连发规则字段（`trigger_key`/`target_key`/`stop_key`）与全局热键字段（`global_toggle`/`global_stop`/`panel_toggle`）都用 `KeyId`，`PENDING_INJECTIONS` 注入事件队列也以 `(KeyId, is_up)` 为键。定义在 `packages/qzh-profile/src/key_id.rs`。

**AES 主密钥**：当前为编译期常量占位符（`packages/crypto/src/aes.rs` 顶部 `MASTER_KEY`），发布前需替换为 build script 注入的真实密钥。

**许可证**：Ed25519 离线校验。私钥仅在 `apps/keygen` 使用，不进主应用二进制。兑换码 `QZHUA-XXXXX-XXXXX-XXXXX-XXXXX`（Base32：64 字节签名 + JSON payload）。payload 含 `issue_time`（防时钟回拨）+ `expiry` + `features u32`（位掩码，见 `license.rs::feature_bits`）。公钥当前为全零占位，发布前替换。

**连发引擎**（`packages/burst-engine`）：`windows_sys` `WH_KEYBOARD_LL` + `WH_MOUSE_LL` 双低级钩子共用同一消息循环线程，监听键盘与鼠标 5 键（左 / 右 / 中 / X1 / X2，含 `WM_XBUTTONDOWN/UP` 高 16 位识别 X1/X2）。按键/按钮注入分三档通道，按用户在设置中选择的优先级生效：

- **SendInput 默认**（`win-input/src/lib.rs`）：键盘 `SendInput INPUT_KEYBOARD` + `KEYEVENTF_SCANCODE`；鼠标 `INPUT_MOUSE` + `MOUSEEVENTF_*` 标志（X1/X2 用 `MOUSEEVENTF_XDOWN/UP` + `mouseData=XBUTTON1/2`）。`dwExtraInfo = SIM_MARKER` 标记自身注入事件防循环。
- **DD 驱动**（`win-input/src/dd_common.rs` + `win-input/src/ddhid.rs`）：动态加载 DD 驱动 DLL，键盘 `DD_key`，鼠标 `DD_btn`（值域 1=L↓/2=L↑/4=R↓/8=R↑/16=M↓/32=M↑，**X1/X2 不在值域**，回退 SendInput）。
- **Interception 驱动**（`win-input/src/interception.rs`）：键盘 + 鼠标设备各扫描一次，鼠标 `InterceptionMouseStroke` 状态位映射 `INTERCEPTION_MOUSE_BUTTON_4/5_DOWN/UP`（X1/X2 走 BUTTON_4/5）。

`win_input::dispatch(KeyId, is_up)` 是统一入口，`(mode, KeyId)` 模式匹配分发到对应 backend，X1/X2 在 DD 模式 / 鼠标设备缺失时按 once 旗标 warn 一次后自动回退 SendInput。`burst-engine` 负责线程编排：用 `catch_unwind` 包裹引擎线程，并发连发用 `AtomicBool cancel + thread::park_timeout`，`Drop` 时先 signal 再 join 确保按键不卡住。非 Windows 平台提供空实现（`cfg(windows)` 隔离）。

全局热键不走 `tauri-plugin-global-shortcut` 注册，而是与连发规则共用 `burst-engine` 低级 hook：热键检测优先于规则处理，且不受 `global_enabled` 当前状态限制。引擎用 `pressed_keys: HashSet<KeyId>` 记录已经按下的物理键，只让首次 down 进入 `on_key_press`，up 时移除；不要再依赖 `KBDLLHOOKSTRUCT.flags` 的保留位判断 key-repeat。注入事件仍先在 hook 层过滤：SendInput / Interception 用 `SIM_MARKER`，DD-HID 用 `PENDING_INJECTIONS`。

**AppHandle 不进 packages**：`win-driver` / `win-input` / `win-sysinfo` / `burst-engine` 所有函数均不接受 `AppHandle` 参数。资源目录由 `commands/driver.rs` 从 `app.path().resource_dir()` 取得后传入，Tauri 状态管理留在 commands 层。

**数据存储路径**：`{app_data_dir}/profiles/`（.qzh）、`{app_data_dir}/settings.json`（plugin-store）、`{app_local_data_dir}/pending_update/`（下载待安装更新包）、`{app_log_dir}/`（rolling logs）。由 Tauri `PathResolver` 跨平台解析。

## 输入约束

| 参数         | 范围           | 执行位置                                      |
| ------------ | -------------- | --------------------------------------------- |
| 连发间隔     | 10ms – 10000ms | `qzh-profile/src/profile.rs::validate()`      |
| 单配置规则数 | ≤ 64           | `qzh-profile/src/profile.rs::validate()`      |
| 宏序列步骤数 | ≤ 256          | `qzh-profile/src/macro_seq.rs::MAX_STEPS`     |

## 功能分层

核心功能：按压连发、Toggle 连发（键盘 + 鼠标 5 键统一支持）、配置文件管理、桌宠基础动画、自动更新。  
亲友专属功能（兑换码激活，`feature_bits` 控制）：宏录制回放、随机抖动、条件配置集、桌宠扩展动画包。`MOUSE_BURST` 位预留但当前不限制——v0.2 鼠标连发对所有用户开放。

## 发版流程

**更新日志**：`CHANGELOG.md`（项目根目录）是唯一内容源，格式为 `## [版本号] - 日期` + 中文分节（新功能 / 问题修复 / 行为变更 / 升级方式 / 已知问题）。CI 发版时由 `scripts/extract-changelog.ts` 自动提取当前版本节作为 GitHub Release 正文，同时作为 `update.body` 通过 `update-ready` 事件在应用内「更新公告」弹窗展示。

**应用名称入口**（改名时同步修改以下位置）：
- 前端：`apps/main/src/constants.ts` — `APP_NAME`（中文）、`APP_NAME_EN`（英文），所有前端代码从此引入
- Rust：`apps/main/src-tauri/src/lib.rs` — `APP_NAME` / `APP_NAME_CN` 常量，Rust 代码引用此处
- 配置：`tauri.conf.json` 的 `productName` / `title`；`apps/main/src-tauri/Cargo.toml` 的 `name`（标识符，轻易不改）

**发版步骤**：
1. 在 `CHANGELOG.md` 的 `[Unreleased]` 节填写本次更新内容
2. 同步更新 `README.md`（面向用户的使用说明书）：核对界面示意图、操作步骤、新增 / 变更功能的描述，确保与新版本实际行为一致
3. 运行 `pnpm bump-version X.X.X`，自动同步三处版本号并将 `[Unreleased]` 重命名为 `[X.X.X] - 日期`（脚本：`scripts/bump-version.ts`）
4. 提交：`chore(release): bump version to X.X.X`
5. 打 tag：`git tag vX.X.X && git push origin main && git push origin vX.X.X`
6. tag 推送后 CI 自动构建、提取 changelog、发布 Draft Release，审查后手动发布

## 协作规范

**commit-msg**（Conventional Commits）：`type(scope): description`  
type：`feat` | `fix` | `docs` | `style` | `refactor` | `test` | `chore` | `ci` | `build` | `perf` | `revert`

**pre-commit**：暂存 `.rs` → `cargo fmt --check` + `cargo clippy -D warnings`；暂存 `.ts/.tsx` → `oxlint` + `oxfmt --check`。

**Workspace lints**：根 `Cargo.toml` 的 `[workspace.lints.clippy]` 是统一 lint 源（当前含 `uninlined_format_args = "warn"`）。新增 crate 的 `Cargo.toml` 必须加 `[lints] workspace = true` 继承，否则 clippy 规则不会生效。

**覆盖率门槛**：`packages/{qzh-format, qzh-profile, crypto, migrate}` 整体行覆盖 ≥ 85%、函数 ≥ 80%、region ≥ 85%。CI 由 `.github/workflows/coverage.yml` 强制（PR 与 push:main 触发）；新增共享 crate 须同步加入 workflow 与 `package.json` 的 `coverage` 脚本的 `--package` 列表。`apps/main/src-tauri` 与 `win-*` / `burst-engine` 因含大量 `#[cfg(windows)]` 代码，不在阈值监控范围。

- 全程使用中文。

- 提交信息不添加 `Co-Authored-By` 署名行。

- 不主动commit，除非用户明确要求。
