# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**气质花（Qizhi Hua）** — 面向游戏辅助的按键助手。基础功能免费，高级功能通过 Ed25519 离线兑换码激活。

详细规划见 `docs/ROADMAP.md`，资源清单见 `docs/ASSETS.md`。

## Monorepo 结构

项目尚未脚手架。目标结构：

```
apps/main/          # 单一 Tauri 应用（面板窗口 + 桌宠窗口 + 系统托盘）
apps/keygen/        # 兑换码生成 CLI（纯 Rust）
apps/release-server/ # 落地页服务（Axum + rust-embed）
packages/crypto/    # AES-256-GCM + HKDF + Ed25519 校验
packages/migrate/   # 通用版本迁移接口（VersionedData trait）
packages/qzh-format/ # .qzh 配置文件格式（加密读写 + schema 迁移）
docs/
```

脚手架完成后常用命令预期为：
```
pnpm dev           # 启动 Tauri 开发模式
pnpm build         # 构建所有产物
cargo test -p <crate>  # 运行指定 crate 的测试
```

## 关键架构决策

**单进程多窗口**：面板（`panel.html`）和桌宠（`pet.html`）是同一 Tauri 进程内的两个独立 WebView，通过 `app.emit_all()` 通信，无 Named Pipe、无独立进程。

**配置文件格式（.qzh）**：二进制头（Magic/Version/Flags/Nonce）+ AES-256-GCM 密文 + Auth Tag。JSON payload 首字段为 `schema_version`，驱动 `packages/qzh-format/src/migrate.rs` 中的迁移链（Strategy B）。应用设置（`tauri-plugin-store`）复用同一迁移基础设施（`packages/migrate`）。

**许可证系统**：Ed25519 离线校验。私钥仅在 `apps/keygen` 中使用，不进主应用二进制。兑换码格式 `QZHUA-XXXXX-XXXXX-XXXXX-XXXXX`（Base32）。payload 含 `issue_time`（防时钟回拨）+ `expiry` + `features u32`（功能位掩码）。

**连发引擎安全**：`rdev` 全局监听 + `enigo` 模拟。用 `AtomicUsize sim_count` 过滤 enigo 自身产生的事件，防止事件循环。引擎在独立线程运行，`catch_unwind` 包裹主循环，panic 后自动重启。

**发布基础设施**：安装包和 updater manifest（`latest.json`）托管在 GitHub Releases。GitHub Actions 使用矩阵构建（`windows-latest` / `macos-latest`），Tauri 不支持交叉编译。仓库设为 public 享受无限 Actions 分钟数。落地页由 `apps/release-server` 自托管，下载链接指向 GitHub Releases。

**数据存储路径**：`{app_data_dir}/profiles/`（.qzh 文件）、`{app_data_dir}/settings.json`（plugin-store）、`{app_log_dir}/`（rolling logs）。路径由 Tauri `PathResolver` 跨平台解析。

## 输入约束（schema 校验层执行）

| 参数 | 范围 |
|------|------|
| 连发间隔 | 10ms – 10000ms |
| 单配置规则数 | ≤ 64 |
| 宏序列步骤数 | ≤ 256 |

## 功能分层

免费：按压连发、Toggle 连发、配置文件管理、桌宠基础动画、自动更新。  
高级（兑换码激活）：宏录制回放、鼠标连点、随机抖动、条件配置集、桌宠扩展动画包。

## 协作规范

克隆后执行一次以启用 git hooks：

```sh
git config core.hooksPath .githooks
```

**commit-msg 规范**（Conventional Commits）：

```
type(scope): description
```

type 取值：`feat` | `fix` | `docs` | `style` | `refactor` | `test` | `chore` | `ci` | `build` | `perf` | `revert`

**pre-commit 检查**：暂存 `.rs` 文件时自动运行 `cargo fmt --check` + `cargo clippy`；暂存 `.ts/.tsx` 文件时运行 `pnpm lint`。无对应文件时跳过。

- 提交信息不添加 `Co-Authored-By` 署名行。
