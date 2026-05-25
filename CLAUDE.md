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
```

克隆后执行一次：

```sh
git config core.hooksPath .githooks
```

## Monorepo 结构

```
apps/main/src-tauri/src/        # Tauri 后端（Rust）
apps/main/src/windows/panel/    # 面板窗口（React）
  main.tsx                      # 入口，挂载 Provider
  PanelApp.tsx / .css           # 根组件
  components/                   # UI 基础组件（Overlay、Toast、ConfirmDialog、ContextMenu、KeyCapture、SvgIcon、icons、CloseBehaviorForm）
  dialogs/                      # 弹窗内容组件（AboutDialog、AgreementDialog、UpdateNoticeDialog）
apps/main/src/assets/icons/     # SVG 图标源文件（currentColor / 1em 尺寸）
apps/keygen/                    # 兑换码生成 CLI
apps/release-server/            # 落地页（Axum，待实现）
packages/crypto/src/
  aes.rs                        # AES-256-GCM encrypt/decrypt
  license.rs                    # Ed25519 verify_license + LicensePayload
packages/migrate/src/lib.rs     # run_migrations() 泛型迁移运行器
packages/qzh-format/src/
  header.rs                     # FileHeader（Magic/Version/Flags/Nonce）
  profile.rs                    # Profile / BurstRule 数据结构 + validate()
  migrate.rs                    # migrate_profile()，调用 packages/migrate
```

## 关键架构决策

**单进程多窗口**：面板（`panel.html`）和桌宠（`pet.html`，v0.3 加入）是同一 Tauri 进程的独立 WebView，通过 `app.emit_all()` 通信，无 Named Pipe。

**配置文件格式（.qzh）**：`FileHeader`（19 字节，含 Nonce）+ AES-256-GCM 密文 + Auth Tag。Header 的 `magic+version+flags` 作为 AAD 防篡改。JSON payload 首字段 `schema_version` 驱动 `qzh-format/src/migrate.rs` 迁移链（Strategy B）。`tauri-plugin-store` 的 settings.json 复用同一迁移基础设施（`packages/migrate`）。

**AES 主密钥**：当前为编译期常量占位符（`packages/crypto/src/aes.rs` 顶部 `MASTER_KEY`），发布前需替换为 build script 注入的真实密钥。

**许可证**：Ed25519 离线校验。私钥仅在 `apps/keygen` 使用，不进主应用二进制。兑换码 `QZHUA-XXXXX-XXXXX-XXXXX-XXXXX`（Base32：64 字节签名 + JSON payload）。payload 含 `issue_time`（防时钟回拨）+ `expiry` + `features u32`（位掩码，见 `license.rs::feature_bits`）。公钥当前为全零占位，发布前替换。

**连发引擎**：`windows_sys` `WH_KEYBOARD_LL` 全局键盘 Hook 监听物理按键，`SendInput` + `KEYEVENTF_SCANCODE`（`MapVirtualKeyW(MAPVK_VK_TO_VSC_EX)`）模拟扫描码输入使游戏可识别。`dwExtraInfo = SIM_MARKER` 标记自身注入事件防循环。引擎线程用 `catch_unwind` 包裹，并发连发用 `AtomicBool cancel + thread::park_timeout`，`Drop` 时先 signal 再 join 确保按键不卡住。非 Windows 平台提供空实现（`cfg(windows)` 隔离）。

**数据存储路径**：`{app_data_dir}/profiles/`（.qzh）、`{app_data_dir}/settings.json`（plugin-store）、`{app_local_data_dir}/pending_update/`（下载待安装更新包）、`{app_log_dir}/`（rolling logs）。由 Tauri `PathResolver` 跨平台解析。

## 输入约束（在 `profile.rs::validate()` 执行）

| 参数         | 范围           |
| ------------ | -------------- |
| 连发间隔     | 10ms – 10000ms |
| 单配置规则数 | ≤ 64           |
| 宏序列步骤数 | ≤ 256          |

## 功能分层

核心功能：按压连发、Toggle 连发、配置文件管理、桌宠基础动画、自动更新。  
亲友专属功能（兑换码激活，`feature_bits` 控制）：宏录制回放、鼠标连点、随机抖动、条件配置集、桌宠扩展动画包。

## 发版流程

**更新日志**：`CHANGELOG.md`（项目根目录）是唯一内容源，格式为 `## [版本号] - 日期` + 中文分节（新功能 / 问题修复 / 行为变更 / 升级方式 / 已知问题）。CI 发版时由 `scripts/extract-changelog.ts` 自动提取当前版本节作为 GitHub Release 正文，同时作为 `update.body` 通过 `update-ready` 事件在应用内「更新公告」弹窗展示。

**应用名称入口**（改名时同步修改以下位置）：
- 前端：`apps/main/src/constants.ts` — `APP_NAME`（中文）、`APP_NAME_EN`（英文），所有前端代码从此引入
- Rust：`apps/main/src-tauri/src/lib.rs` — `APP_NAME` / `APP_NAME_CN` 常量，Rust 代码引用此处
- 配置：`tauri.conf.json` 的 `productName` / `title`；`apps/main/src-tauri/Cargo.toml` 的 `name`（标识符，轻易不改）

**发版步骤**：
1. 在 `CHANGELOG.md` 的 `[Unreleased]` 节填写本次更新内容
2. 运行 `pnpm bump-version X.X.X`，自动同步三处版本号并将 `[Unreleased]` 重命名为 `[X.X.X] - 日期`（脚本：`scripts/bump-version.ts`）
3. 提交：`chore(release): bump version to X.X.X`
4. 打 tag：`git tag vX.X.X && git push origin main && git push origin vX.X.X`
5. tag 推送后 CI 自动构建、提取 changelog、发布 Draft Release，审查后手动发布

## 协作规范

**commit-msg**（Conventional Commits）：`type(scope): description`  
type：`feat` | `fix` | `docs` | `style` | `refactor` | `test` | `chore` | `ci` | `build` | `perf` | `revert`

**pre-commit**：暂存 `.rs` → `cargo fmt --check` + `cargo clippy -D warnings`；暂存 `.ts/.tsx` → `oxlint` + `oxfmt --check`。

- 全程使用中文。

- 提交信息不添加 `Co-Authored-By` 署名行。

- 不主动commit，除非用户明确要求。
