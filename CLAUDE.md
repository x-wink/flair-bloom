# CLAUDE.md

**气质花（FlairBloom）** — 面向游戏辅助的按键助手，Tauri v2 + React，Rust 后端跑全局键鼠钩子与注入。核心功能免费，亲友专属功能通过 Ed25519 离线兑换码激活。

项目文档的单一来源是仓内 skill [`skills/flair-bloom/SKILL.md`](skills/flair-bloom/SKILL.md)，本文件只做索引与硬约束一览，不复制正文。`README.md` 是面向用户的说明书首屏，`CHANGELOG.md` 是版本变化的唯一内容源，`docs/roadmaps/` 放变更路线图。

## 常用命令

```sh
pnpm dev / pnpm build           # Tauri 开发模式 / 生产包
pnpm lint / pnpm format:check   # oxlint / oxfmt（范围 apps/main/src）
pnpm skills:check               # 教程 ↔ 说明书一一对应 + 入口文件链接存在
cargo check / cargo clippy --all-targets --all-features -- -D warnings
cargo test -p <crate>           # 如 -p crypto
pnpm coverage                   # 四个共享 crate 覆盖率（CI 同源）
```

克隆后执行一次 `git config core.hooksPath .githooks`；技能软链重建见 [development](skills/flair-bloom/references/engineering/development.md)。

## 按任务读取

| 任务                                                                 | 读取                                                                                                   |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| 整体结构、窗口与事件、输入模式与注入通道、引擎、热键、存储路径、日志 | [architecture](skills/flair-bloom/references/engineering/architecture.md)                              |
| 按键录入、热键约束、槽位策略、配置装载净化                           | [key-policy](skills/flair-bloom/references/engineering/key-policy.md)                                  |
| `.qzh`、schema 迁移、输入约束、settings.json、协议记录、许可证       | [profile-format](skills/flair-bloom/references/engineering/profile-format.md)                          |
| 自动更新、更新公告、Markdown 渲染、发布基础设施与镜像                | [updater](skills/flair-bloom/references/engineering/updater.md)                                        |
| 发版、CHANGELOG、改应用名、回退                                      | [release](skills/flair-bloom/references/engineering/release.md)                                        |
| 本地环境、目录结构、协作规范、文档约定                               | [development](skills/flair-bloom/references/engineering/development.md)                                |
| 写测试、看哪层已自动化、真机冒烟怎么跑                               | [testing](skills/flair-bloom/references/engineering/testing.md)                                        |
| 改用户可见行为、写说明、改新手教程                                   | `skills/flair-bloom/references/manual/<教程 id>.md` + `apps/main/src/windows/panel/tour/tours/<id>.ts` |
| 真机验证、操作应用                                                   | 先读 `skills/flair-bloom/references/manual/*.md` 按说明书操作，在测试配置里做，不从源码反推点击路径    |
| 规划一个改动                                                         | `docs/roadmaps/<change>.md`（Status / 决策 / 任务卡 / 工期），完成归档到 `archive/`                    |

## 本仓硬约束

- **发版一次一授**：推 tag 触发真发布前必须获得用户明确授权，上次授权不外推；本项目经 Tauri updater 触达真实用户，全局约定的「演示项目免逐次授权」例外**不适用**。
- **高风险版本禁止裸 bump `CURRENT_SCHEMA_VERSION`**：新增字段一律 `#[serde(default)]`；updater 只升不降，schema bump 后向前滚修复会砸用户配置。旧 GitHub Release 不删除。
- **AppHandle 不进 packages**：`win-*` / `burst-engine` 不接受 `AppHandle`，Tauri 状态管理留在 commands 层。
- **所有配置装载必经 `activate_profile_file`**，由它统一 `sanitize_profile`；`Profile::validate()` 刻意不查按键。
- **文档同步在同一提交**：用户可见行为变了改 `manual/<id>.md` 与对应教程文件，架构事实变了改 `engineering/*.md`；pre-commit 在改 `.md`、`skills/`、`tour/` 时跑 `pnpm skills:check`。
- **新增 crate** 必须加 `[lints] workspace = true`；新增共享 crate 同步加入 `coverage.yml` 与 `package.json` 的 `coverage` 脚本。
- **不描述变更史**：文档只写现状与原因，版本变化只进 `CHANGELOG.md`（`[Unreleased]` 记最终净变化）。

通用协作约定（语言、提交与推送策略、注释风格、危险动作授权）以全局 `~/.claude/CLAUDE.md` 为单一事实来源，本文件不重复。
