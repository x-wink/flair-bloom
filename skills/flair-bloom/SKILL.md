---
name: flair-bloom
description: 气质花（FlairBloom）按键助手的项目文档单一来源：用户说明书大纲（与应用内六组新手教程一一对应）与工程事实（架构、按键策略、数据格式、更新、发版、开发规范）。按任务读取对应 reference，不整套加载。
---

# 气质花 · 项目文档

本 skill 是仓库文档的唯一事实源。`CLAUDE.md` 与 `README.md` 只做索引与首屏，不复制正文；`docs/` 下只放路线图与专项 runbook。先以源码与配置确认当前事实，需要背景时再读对应 reference；改动涉及的事实必须在同一提交里同步这里。

## 按任务读取

| 任务                                                                     | 读取                                                                                                     |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| 理解整体结构、窗口与事件、输入模式与注入通道、引擎、热键、存储路径、日志 | [engineering/architecture.md](references/engineering/architecture.md)                                    |
| 改按键录入、热键约束、槽位策略、配置装载净化                             | [engineering/key-policy.md](references/engineering/key-policy.md)                                        |
| 改 `.qzh`、schema 迁移、输入约束、settings.json、协议记录、许可证        | [engineering/profile-format.md](references/engineering/profile-format.md)                                |
| 改自动更新、更新公告、Markdown 渲染、发布基础设施与镜像                  | [engineering/updater.md](references/engineering/updater.md)                                              |
| 发版、写 CHANGELOG、改应用名、回退                                       | [engineering/release.md](references/engineering/release.md)（推 tag 前一次一授）                         |
| 本地环境、命令、目录结构、协作规范、文档约定                             | [engineering/development.md](references/engineering/development.md)                                      |
| 写测试、看哪层已自动化、真机冒烟怎么跑                                   | [engineering/testing.md](references/engineering/testing.md)                                              |
| 改用户可见行为、写说明、改新手教程                                       | `references/manual/<教程 id>.md`（下表），同时改 `apps/main/src/windows/panel/tour/tours/<id>.ts`        |
| 真机验证、操作应用（加规则、录键、切模式、切配置、改设置）               | 先读对应的 `references/manual/*.md` 按说明书操作，不从源码反推点击路径；在测试配置里做（见 development） |

## 说明书（与应用内教程一一对应）

| id                | 篇                                                | 内容                                             |
| ----------------- | ------------------------------------------------- | ------------------------------------------------ |
| `getting-started` | [上手三步](references/manual/getting-started.md)  | 添加第一条规则、录键、打开总开关（按教程版本自动进入） |
| `game-mode`       | [游戏模式与驱动](references/manual/game-mode.md)  | 为什么游戏里要装驱动、怎么装、装完做什么         |
| `rules`           | [规则玩法](references/manual/rules.md)            | 筛选、规则卡、点标签换模式、高级设置、冲突提醒   |
| `groups`          | [互斥组与多段宏](references/manual/groups.md)     | 同组只跑一条：切换是换人、长按是插队、示例组实操 |
| `layouts`         | [横版键鼠图与浮窗](references/manual/layouts.md)  | 横版点键三态、图例、统一间隔、收进浮窗           |
| `profiles`        | [多套配置](references/manual/profiles.md)         | 新建 / 切换、导入导出 `.qzh`、托盘切换           |
| `settings`        | [热键、声音与外观](references/manual/settings.md) | 全局热键、语音播报、主题、关闭行为与自启         |

每篇的「步骤大纲」表是教程文件的规格：锚点、讲什么、实操判定。写步骤时守一条：文案里让用户做的动作（点、勾、按）必须是实操步（有 `done`），讲解步会铺一层遮挡、高亮的目标也点不动，让用户去点等于卡死。`pnpm skills:check` 校验教程 id 与篇名一一对应、三个入口文件的相对链接存在，pre-commit 在改 `.md`、`skills/`、`tour/` 时自动跑。

## 事实边界

- 后端命令签名、事件名、Token 名以源码为准；这里不复制命令清单。
- `CHANGELOG.md` 是版本变化的唯一内容源，这里不写变更史。
- `docs/roadmaps/*.md` 是规划与决策记录（active / draft / archive），不是现状描述。
- 产品页 `apps/site` 有自己的 README 与文案副本（`app/utils/content.ts`），本 skill 不覆盖它。
