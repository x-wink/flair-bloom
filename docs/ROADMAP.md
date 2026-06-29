# 气质花 — Tauri 按键助手 路线图

## 项目概述

面向游戏辅助的按键助手。核心功能免费开放，亲友专属功能通过兑换码离线激活使用时长。
Monorepo 结构，Rust workspace + pnpm workspace 双层管理。
目标支持面板、托盘、桌宠三种运行模式，均在单一 Tauri 进程内管理。当前主线为 Windows 桌面端，已实现面板窗口 + 系统托盘常驻运行，桌宠模式进入 v0.4 规划。

## 当前阶段（2026-06-29）

- 当前基线：`v0.3.0`。已发布 0.2.0–0.2.9；v0.3.0 为本次发布，主题为体验完善、稳定性与完整配置管理。
- 已完成主能力：按压 / Toggle 连发、键盘全键位、鼠标 5 键、滚轮连发、Toggle 互斥分组（同组互斥切换）、规则拖拽排序与分组折叠/展开、竖版规则列表 + 横版键鼠图双布局、常驻置顶浮窗（显示激活规则 + 全局开关 + 展开主面板）、主题换肤（21 门派配色 + 亮 / 暗 / 跟随系统暗色模式）、多输入模式（主推游戏模式 Interception；通用 SendInput 与 DD驱动 DdSimple 备用；DDHID 已禁用；DD 系列与横版互斥）、多配置文件、全局热键（仅键盘、互不重复）、面板显隐热键、设置面板、声音反馈（每项独立开关）、诊断修复、外部配置导入、自动更新、系统托盘、开机自启。
- 连发速率：注入周期有效下限 `MIN_EFFECTIVE_INTERVAL_MS = 10ms`（结构下限仍为 1ms，旧配置 <10ms 在加载时自动钳到 10ms）；多规则同时连发时按「基础下限 × 活跃规则数」做总并发限速，使总注入速率与规则数无关，避免叠加超发导致停止「收不住」。
- 配置 schema：`CURRENT_SCHEMA_VERSION = 4`。v1→v2 裸 VK 升级为 `KeyId`；v2→v3 增加滚轮上 / 下；v3→v4 `BurstRule` 新增可选 `group` 字段（Toggle 互斥分组）。
- 用户协议：`v1.3`（补充 DD驱动模式、DDHID 暂停、驱动残留与卸载说明）。
- 后续主线：v0.3 体验与稳定性收尾（本次 v0.3.0 落地浮窗 / 横版键鼠图 / 主题换肤）、v0.4 桌宠、v0.5 许可证与亲友专属功能、v0.6 落地页与运营基础、v1.0 完整功能。下一阶段聚焦体验打磨与遗留收尾，暂不开新大模块。
- 进度标记：`[x]` 表示当前代码或发布流程已具备，`[ ]` 表示仍在规划或未完成。

---

## 目录

> 快速跳转：[迭代计划](#迭代计划) · [技术选型](#技术选型) · [功能分层](#功能分层) · [风险与缓解](#风险与缓解已确认)

1. [架构设计准则](#架构设计准则)
2. [应用模式](#应用模式)
3. [Monorepo 目录结构](#monorepo-目录结构)
4. [架构说明](#架构说明)
5. [桌宠模式设计](#桌宠模式设计)
6. [用户协议设计](#用户协议设计)
7. [功能分层](#功能分层)
8. [配置文件格式（.qzh）](#配置文件格式qzh)
9. [许可证系统](#许可证系统ed25519-离线校验)
10. [**迭代计划**](#迭代计划)
11. [技术选型](#技术选型)
12. [风险与缓解](#风险与缓解已确认)

---

## 架构设计准则

### 一、小白友好，开箱即用

**目标：** 用户安装后不看文档也能上手，5 分钟内配好第一条规则。

- **合理默认值**：所有参数预设最常用值（连发间隔默认 50ms，触发方式默认按压），用户无需改动即可使用
- **新手引导**：首次启动（协议同意后）展示简短引导流程，引导创建第一条规则
- **规则模板**：内置常用场景模板（FPS 快速连发、MOBA 技能连按等），一键导入即可使用
- **渐进式界面**：基础视图只显示核心操作，高级选项折叠隐藏，不让初级用户感到困惑
- **错误提示友好**：所有报错用自然语言描述（"配置文件已损坏，是否恢复默认？"），不暴露技术细节

### 二、灵活扩展，充分可配置

**目标：** 高级用户能自定义每一个细节，不受默认值束缚。

- **所有行为均可配置**：连发间隔、触发方式、热键、桌宠位置、动画开关、日志级别等全部暴露设置项
- **配置文件可导入导出**：`.qzh` 格式跨设备迁移，支持社区分享配置
- **高级设置面板**：独立页面集中管理全局选项，与主配置分离，不影响基础操作
- **功能开关**：桌宠动画、输入响应、开机自启等均可独立关闭，满足不同使用偏好

### 三、容错重试，稳健运行

**目标：** 单个功能出错不影响整体运行，关键操作失败后自动恢复。

| 场景                          | 容错策略                                                          |
| ----------------------------- | ----------------------------------------------------------------- |
| 连发循环线程 panic            | `catch_unwind` 捕获，记录日志，补发 `key_up`，避免按键卡住       |
| 配置文件读取失败（损坏/篡改） | 提示用户，提供"恢复默认配置"选项，不阻塞启动                      |
| `notify` 文件监听失效         | 规划项；当前暂未接入配置文件监听                                  |
| 自动升级下载失败              | 前端展示下载失败提示，不影响正常使用                              |
| 全局监听或输入后端初始化失败  | 提示用户切换模式 / 以管理员权限运行，其余功能保持可用            |
| 规则热键冲突                  | 检测到冲突时在界面标注风险并提示用户调整                          |
| Tauri Command 异常            | 所有 Command 返回 `Result`，前端统一错误处理，显示 toast 提示     |

**引擎线程隔离：** 连发循环在独立线程运行，panic 不传播到主进程，`std::panic::catch_unwind` 包裹具体连发循环并做按键释放兜底。

### 四、日志完善，崩溃可追溯

**目标：** 出问题时用户能一键提供有效日志，开发者能快速定位问题。

#### 日志系统

- **Rust 端**：`tracing` + `tracing-subscriber` + `tracing-appender`
  - 按天滚动日志文件，保留最近 7 天
  - 日志路径：Windows `%LOCALAPPDATA%\fun.xwink.flairbloom\logs\flair-bloom.YYYY-MM-DD`；macOS `~/Library/Logs/fun.xwink.flairbloom`
  - 级别：`ERROR` / `WARN` / `INFO` / `DEBUG`（当前默认 INFO，后续可开放运行时切换）
- **前端**：JS 错误通过 Tauri Command 转发到 Rust logger，统一写入同一日志文件
- **结构化格式**：每条日志含时间戳、级别、模块路径、线程 ID

#### 崩溃处理

```
std::panic::set_hook → 捕获 panic 信息
  → 写入 crash-{unix_ts}.log（独立崩溃日志）
  → 后续补充崩溃提示窗口（Tauri 原生对话框）
      ┌─────────────────────────────────────┐
      │ 气质花遇到了一个问题并已崩溃          │
      │                                     │
      │ 崩溃日志已保存，如需报告问题请提供：  │
      │ C:\Users\...\fun.xwink.flairbloom\logs\crash-... │
      │                                     │
      │ [打开日志文件夹]  [复制日志路径]      │
      │              [确定关闭]              │
      └─────────────────────────────────────┘
```

#### 用户反馈引导

- 关于 / 诊断入口提供"打开日志目录"入口（日常排查用）
- 崩溃弹窗提供"复制日志路径"按钮（一键复制，方便粘贴给开发者，待实现）
- 日志文件夹直接用系统文件管理器打开，降低操作门槛
- 日志文件不含任何用户个人信息（无硬件 ID、无用户名），用户无需顾虑隐私

#### 日志分级规范

| 级别  | 使用场景                                            |
| ----- | --------------------------------------------------- |
| ERROR | 功能不可用的严重错误（引擎崩溃、文件损坏）          |
| WARN  | 降级运行的异常（重试成功、配置回退）                |
| INFO  | 关键状态变更（连发启动/停止、配置切换、许可证激活） |
| DEBUG | 详细运行信息（每次按键事件、IPC 消息），默认关闭    |

---

## 应用模式

### 面板模式（Panel）

全功能配置界面，适合初始设置和规则管理。标题栏最小化走系统最小化；关闭按钮可按用户偏好选择退出或隐藏到托盘；面板显隐热键使用最小化 / 恢复语义，方便从任务栏手动唤回。

### 托盘模式（Tray）

面板隐藏到托盘后，Tauri 进程继续常驻，连发功能保持有效。托盘菜单当前提供全局开关、打开面板、退出；托盘双击和再次启动应用都会唤回面板。

### 桌宠模式（Pet）

v0.4 规划项，当前尚未创建 `pet.html` 与对应窗口。目标是透明无边框、始终置顶的小窗口，通过动画反映连发状态；默认点击穿透，鼠标悬停时临时关闭穿透以支持拖拽和右键菜单。

---

## Monorepo 目录结构

```
气质花/
├── Cargo.toml                          # Rust workspace 根
├── pnpm-workspace.yaml                 # pnpm workspace
├── package.json                        # 根脚本（dev / build / release）
│
├── apps/
│   ├── main/                           # 单一 Tauri 应用（面板 + 托盘；v0.4 加入桌宠）
│   │   ├── package.json
│   │   ├── panel.html                  # 面板窗口入口（已实现）
│   │   ├── pet.html                    # 桌宠窗口入口（v0.4 规划）
│   │   ├── src/
│   │   │   ├── windows/
│   │   │   │   ├── panel/              # 面板窗口 UI
│   │   │   │   │   ├── components/     # 基础 UI（Overlay、Toast、ConfirmDialog、ContextMenu、KeyCapture、SvgIcon、icons、CloseBehaviorForm）
│   │   │   │   │   ├── dialogs/        # 设置 / 关于 / 协议 / 更新公告 / 诊断修复 / 导入
│   │   │   │   │   ├── main.tsx
│   │   │   │   │   └── PanelApp.tsx
│   │   │   │   └── pet/                # 桌宠窗口 UI（v0.4 规划）
│   │   │   │       ├── components/
│   │   │   │       │   ├── PetCanvas/
│   │   │   │       │   └── StatusBubble/
│   │   │   │       ├── hooks/
│   │   │   │       │   ├── useEngineStatus.ts
│   │   │   │       │   └── usePetAnim.ts
│   │   │   │       └── PetApp.tsx
│   │   │   └── assets/
│   │   │       └── EULA.md             # 用户协议正文（内嵌构建）
│   │   └── src-tauri/
│   │       ├── Cargo.toml              # 依赖 qzh-profile、win-* 等 packages
│   │       ├── tauri.conf.json         # 窗口配置（当前 panel；v0.4 加 pet）
│   │       ├── tauri.windows.conf.json # Windows 打包资源与 NSIS hook
│   │       └── src/
│   │           ├── main.rs
│   │           ├── lib.rs              # Tauri Builder 薄壳
│   │           ├── bootstrap/          # 启动期装配（logging/agreement/update/profile/input）
│   │           ├── tray.rs             # 系统托盘图标与菜单
│   │           ├── engine/
│   │           │   └── mod.rs          # re-export burst-engine / win-input 公开 API
│   │           └── commands/
│   │               ├── app.rs          # 协议同意 / 检查更新 / 退出
│   │               ├── ddhid_diagnostic.rs # DDHID 诊断报告（暂停模式残留排查）
│   │               ├── driver.rs       # 驱动安装卸载 + 提权重启
│   │               ├── engine.rs       # 规则 CRUD + 输入模式切换
│   │               ├── import_profile.rs
│   │               ├── log.rs          # 前端日志转发 + 打开日志目录
│   │               ├── profile.rs      # 配置文件 CRUD
│   │               ├── repair.rs       # 诊断修复
│   │               ├── resource_integrity.rs
│   │               └── status.rs       # 应用状态快照
│   │
│   ├── keygen/                         # 兑换码生成 CLI（v0.5 完整实现）
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   │
│   └── release-server/                 # 落地页服务（v0.6 规划，文件托管在 GitHub Releases）
│       ├── Cargo.toml
│       ├── config.toml                 # 端口、GitHub repo 信息、站点基础信息
│       ├── content/
│       │   └── changelog.toml          # 可由 CHANGELOG.md 转换生成，避免第二内容源
│       ├── static/                     # 静态资源（rust-embed 内嵌到二进制）
│       │   ├── index.html              # 落地页（介绍 + 下载入口 → 跳转 GitHub Releases）
│       │   ├── download.html           # 下载页（展示最新版本，链接指向 GitHub Releases）
│       │   ├── changelog.html          # 更新日志页
│       │   └── css/
│       │       └── style.css
│       └── src/
│           ├── main.rs
│           ├── routes/
│           │   ├── pages.rs            # GET / /download /changelog → HTML 页面
│           │   └── health.rs           # GET /health
│           └── changelog.rs            # 解析更新日志数据，渲染日志页
│
└── packages/
    ├── crypto/                         # 加密解密 + 许可证校验（Rust lib crate）
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── aes.rs
    │       └── license.rs
    │
    ├── migrate/                        # 通用版本迁移接口（qzh-format / settings 共用）
    │   ├── Cargo.toml
    │   └── src/
    │       └── lib.rs                  # VersionedData trait + migrate() 泛型函数
    │
    ├── qzh-format/                     # .qzh 文件容器（header + read/write_encrypted）
    │   └── src/
    │       ├── header.rs               # FileHeader（Magic/Version/Flags/Nonce）
    │       └── lib.rs                  # read_encrypted / write_encrypted
    │
    ├── qzh-profile/                    # 业务 Profile schema（从 qzh-format 独立）
    │   └── src/
    │       ├── key_id.rs               # KeyId（Keyboard(VK) | Mouse(MouseButton)）
    │       ├── profile.rs              # Profile / BurstRule + validate()
    │       ├── macro_seq.rs            # MacroSequence / MacroStep
    │       ├── schema_migrate.rs       # migrate_profile()
    │       └── lib.rs                  # load_from_path / save_to_path
    │
    ├── win-sysinfo/                    # Windows 系统信息 + 注册表 + 安装前置检测
    ├── win-input/                      # SendInput / Interception / DD驱动(DdSimple) / DDHID 输入注入
    ├── burst-engine/                   # BurstEngine + LL keyboard/mouse hook
    ├── win-driver/                     # 驱动安装卸载 + ShellExecuteExW + PowerShell
    └── resource-integrity/             # 打包资源完整性校验
```

---

## 架构说明

### 进程模型

```
┌──────────────────────────────────────────────────────────┐
│  FlairBloom.exe（单一 Tauri 进程）                        │
│                                                          │
│  ┌─────────────────┐   Tauri events      ┌────────────┐ │
│  │  Rust 后端       │ ──────────────────→ │ 面板窗口   │ │
│  │  - 连发引擎      │                     │ WebView    │ │
│  │  - 系统托盘      │ ──────────────────→ │ 桌宠窗口   │ │
│  │  - 更新检查      │                     │ WebView    │ │
│  │  - 驱动诊断      │                     └────────────┘ │
│  │  - 许可证校验    │                                    │
│  └─────────────────┘                                     │
└──────────────────────────────────────────────────────────┘
               │ HTTPS
       ┌───────┴────────┐
       │ GitHub Releases │
       │ release-server  │
       └────────────────┘
```

进度：面板窗口、托盘、更新检查、驱动诊断已落地；桌宠窗口、许可证激活和 release-server 分别进入 v0.4 / v0.5 / v0.6。

### 发布基础设施

| 职责               | 平台                     | 说明                                           |
| ------------------ | ------------------------ | ---------------------------------------------- |
| 安装包托管         | GitHub Releases          | `.exe` / `.nsis.zip` + `.sig` 签名文件         |
| Tauri updater 端点 | GitHub Releases          | 每次发布上传 `latest.json`（updater manifest） |
| 私钥存储           | GitHub Actions Secrets   | Tauri 签名私钥、Ed25519 许可证私钥，构建 / 签发时注入 |
| CI/CD 构建         | GitHub Actions           | 推 tag 触发 Windows x64 构建，发布 Draft       |
| Release 正文       | `CHANGELOG.md`           | `scripts/extract-changelog.ts` 自动提取版本节  |
| 落地页             | release-server（自托管） | v0.6 规划；仅渲染 HTML，下载链接指向 GitHub Releases |

### Tauri updater 配置

```json
// tauri.conf.json
{
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/{owner}/{repo}/releases/latest/download/latest.json"
      ],
      "pubkey": "（Tauri 签名公钥）"
    }
  }
}
```

每次 GitHub Actions 发布时由 `tauri-action` 生成并上传 `latest.json`（Tauri 标准 updater manifest 格式）。

### GitHub Actions 发布流程

```
推送 tag（v1.2.0）
  → actions/checkout
  → 安装 pnpm / Node.js 22 / Rust stable（x86_64-pc-windows-msvc）
  → pnpm install
  → pnpm check:resources
  → 解析上一稳定版本 tag
  → scripts/extract-changelog.ts 提取当前 tag 的 CHANGELOG 版本节
  → tauri-apps/tauri-action 构建并创建 Draft Release
      ├── Windows x64 NSIS 安装包
      ├── updater manifest / 签名文件
      └── Release 正文（来自 CHANGELOG.md）
```

注：当前 release workflow 只构建 Windows x64；v1.0 前评估 macOS / Linux 构建矩阵。

### release-server 路由（v0.6 规划）

| 路由             | 用途                                                      |
| ---------------- | --------------------------------------------------------- |
| `GET /`          | 落地页（介绍 + 功能 + 截图 + 下载按钮 → GitHub Releases） |
| `GET /download`  | 下载页（最新版本信息，链接指向 GitHub Releases）          |
| `GET /changelog` | 更新日志页（由 `CHANGELOG.md` 转换 / 提取）               |
| `GET /health`    | 健康检查                                                  |

---

**单进程多窗口**：面板和桌宠是同一 Tauri 应用的两个独立 WebView 窗口，各自有独立的 HTML 入口（`panel.html` / `pet.html`），通过 Tauri 事件系统通信。当前进度为 `panel` 已实现，`pet` 在 v0.4 加入。

**窗口配置（tauri.conf.json）：**

```json
{
  "windows": [
    {
      "label": "panel",
      "url": "/panel.html",
      "visible": false,
      "width": 405,
      "height": 720,
      "resizable": false,
      "maximizable": false,
      "decorations": false,
      "transparent": true
    },
    {
      "label": "pet",
      "url": "/pet.html",
      "transparent": true,
      "decorations": false,
      "alwaysOnTop": true,
      "skipTaskbar": true,
      "visible": false,
      "width": 160,
      "height": 160
    }
  ]
}
```

### 事件通信

```
引擎状态变更（Rust）
  → app.emit("global-enabled-changed", payload)
  → app.emit("app-status-changed", payload)
  → app.emit("update-*", payload)
  → 面板窗口更新状态显示；激活态规则当前由前端轮询 `get_active_rules`
  → v0.4 桌宠窗口再抽象统一状态事件源
```

### Crate 依赖图

```
packages/crypto      packages/migrate
         ↑                ↑   ↑
         └────────────────┘   │
    packages/qzh-format       │
         ↑                    │
         └────────────────────┘
    apps/main/src-tauri      apps/keygen

packages/win-input  ← packages/burst-engine ← apps/main/src-tauri
packages/win-driver / win-sysinfo / resource-integrity ← apps/main/src-tauri

apps/release-server（v0.6：axum / tokio / rust-embed）
```

### 数据存储路径约定

| 数据类型           | 路径                                        | 说明                                                                       |
| ------------------ | ------------------------------------------- | -------------------------------------------------------------------------- |
| 配置文件（`.qzh`） | `{app_data_dir}/profiles/`                  | 用户导入/导出通过文件对话框，不暴露内部路径                                |
| 应用设置           | `{app_data_dir}/settings.json`              | `tauri-plugin-store` 管理；复杂 settings 迁移待接入 `packages/migrate` |
| 待安装更新包       | `{app_local_data_dir}/pending_update/`      | 静默下载完成后暂存，下次启动自动安装                                       |
| 日志               | `log_dir()`                                 | Windows: `%LOCALAPPDATA%\fun.xwink.flairbloom\logs`；macOS: `~/Library/Logs/fun.xwink.flairbloom` |
| 崩溃日志           | `log_dir()/crash-{unix_ts}.log`             | panic hook 写入                                                            |

`app_data_dir` / `app_local_data_dir` 由 Tauri `PathResolver` 跨平台解析；日志目录由 `bootstrap/logging.rs` 显式定义。

### 输入参数约束

| 参数         | 有效范围       | 说明                                                |
| ------------ | -------------- | --------------------------------------------------- |
| 连发间隔     | 1ms – 10000ms  | UI / 后端统一 clamp；默认 10ms，低于 5ms 视为极速设置并提示风险 |
| 单配置规则数 | ≤ 64 条        | 线性匹配不成瓶颈，上限防止滥用                      |
| 宏序列步骤数 | ≤ 256 步       | 防止回放时间过长                                    |

约束在 `qzh-profile` 的 schema 校验层执行（clamp 或 reject），不依赖前端验证。

### 卸载清理策略

| 数据                        | 策略                           | 说明                                                              |
| --------------------------- | ------------------------------ | ----------------------------------------------------------------- |
| 用户配置（`.qzh`）          | 保留                           | 符合 Windows 惯例，重装后数据仍在                                 |
| 应用设置（`settings.json`） | 保留                           | 同上                                                              |
| 日志文件                    | 保留                           | 用户可手动清理                                                    |
| 注册表自启动项              | 由 installer 清除              | NSIS/MSI 卸载脚本负责移除 `tauri-plugin-autostart` 写入的注册表项 |
| v1.0 可选                   | 卸载时询问是否同时删除配置数据 | 不强制，给用户选择                                                |

---

## 桌宠模式设计

本节是 v0.4 设计稿，当前代码库尚未包含桌宠窗口入口。

### 点击穿透

使用 Tauri 内置 `Window::set_ignore_cursor_events(bool)`（跨平台，Windows / macOS 均支持）。

跨平台鼠标位置监听方案待定，优先使用 Tauri / 系统 API；避免为桌宠重新引入与连发引擎无关的全局输入依赖。监听逻辑用于计算光标是否进入桌宠窗口区域：

- 进入 → `set_ignore_cursor_events(false)` 关闭穿透（支持拖拽和右键）
- 离开 → `set_ignore_cursor_events(true)` 恢复穿透

### 动画状态机

| 状态  | 动画描述               | 触发条件                  |
| ----- | ---------------------- | ------------------------- |
| Idle  | 缓慢呼吸，偶尔眨眼     | 默认                      |
| Burst | 快速抖动或奔跑循环     | 连发引擎激活              |
| Hover | 抬头看向光标，尾巴摇动 | 鼠标进入窗口区域          |
| Alert | 耳朵竖起，眼睛放大     | 切换配置文件              |
| Sleep | 闭眼 ZZZ               | 空闲超过 N 分钟（可配置） |

### 交互行为

- **拖拽**：关闭穿透后鼠标按下拖动，位置存入 `tauri-plugin-store`，重启后恢复
- **右键菜单**：开关连发 / 切换配置文件 / 打开面板 / 退出
- **左键单击**：显示状态气泡（当前规则、许可证到期），3 秒淡出

### 动画资源

MVP 阶段用 CSS 动画 + SVG，后期视美术资源情况升级为 Sprite Sheet 或 Lottie。

### 扩展模式：输入响应动画（最低优先级）

参考 Dongocat，监听全局键盘、鼠标、手柄输入做出对应动画反馈，与连发引擎解耦。

| 状态          | 触发条件                |
| ------------- | ----------------------- |
| Typing        | 连续键盘输入（>2次/秒） |
| KeyPress      | 单次按键                |
| MouseMove     | 鼠标移动（眼睛跟随）    |
| Click         | 鼠标点击（眨眼）        |
| GamepadButton | 手柄按键                |
| GamepadStick  | 摇杆偏移（身体倾斜）    |

手柄监听通过 `gilrs` crate 实现，列入「待定（最低优先级）」，需配套美术资源后再评估。

---

## 用户协议设计

### 触发时机

应用启动时优先检查协议状态，未同意则面板窗口显示协议页，屏蔽所有其他路由。

```
启动
  → 读取 store → { agreed, agreement_version }
  → 未同意 或 版本不匹配
      → 面板窗口展示协议页
      → 同意 → 写入存储 → 进入正常流程
      → 拒绝 → 提示"不同意则无法使用本软件" → 退出
  → 已同意且版本匹配 → 正常启动
```

### 存储记录

```json
{
  "agreed": true,
  "agreed_at": 1748000000,
  "agreement_version": "1.3",
  "app_version_at_agree": "0.2.4"
}
```

### 协议核心条款

| 条款     | 内容                                                               |
| -------- | ------------------------------------------------------------------ |
| 使用风险 | 模拟输入可能触发游戏反作弊机制，导致账号封禁，用户自行承担全部风险 |
| 适用范围 | 仅供个人娱乐与技术学习使用                                         |
| 禁止商用 | 严禁用于任何商业目的                                               |
| 免责声明 | 因使用本软件导致的任何损失，开发者不承担责任                       |
| 知识产权 | 未经授权不得反编译、修改或二次分发                                 |

协议正文滚动到底部才激活同意按钮。`agreement_version` 与代码硬编码版本不一致时强制重新同意。

---

## 功能分层

### 核心功能

| 功能                     | 实现位置                                        |
| ------------------------ | ----------------------------------------------- |
| 用户协议（首次启动）     | apps/main — AgreementDialog + bootstrap/agreement.rs |
| 按压连发                 | packages/burst-engine                           |
| 一键连发（热键 Toggle）  | packages/burst-engine                           |
| 键盘 / 鼠标 / 滚轮连发   | packages/burst-engine + packages/win-input      |
| Toggle 互斥分组          | packages/qzh-profile（`BurstRule.group`）+ packages/burst-engine |
| 规则拖拽排序 / 分组折叠  | 面板 UI（规则列表 + 分组容器）                  |
| 多输入模式（通用 / 游戏 / DD驱动；DDHID 暂停） | packages/win-input + apps/main/src-tauri/commands/engine.rs |
| 配置文件 CRUD            | apps/main — commands/profile.rs                 |
| 外部配置导入             | apps/main — commands/import_profile.rs + ImportDialog |
| `.qzh` 加密格式          | packages/qzh-format + packages/qzh-profile      |
| 快速切换配置文件         | 面板 UI + 设置面板配置卡片                      |
| 设置面板                 | SettingsDialog（通用 / 热键 / 声音 / 配置文件） |
| 诊断修复                 | commands/repair.rs + RepairDialog               |
| 系统托盘 & 开机自启      | apps/main/src-tauri — tray.rs                   |
| 自动升级                 | apps/main — bootstrap/update.rs + commands/app.rs |

### 亲友专属功能（兑换码激活，限时）

| 功能           | 实现位置                                        |
| -------------- | ----------------------------------------------- |
| 宏录制与回放   | apps/main/src-tauri — engine/macro_play.rs      |
| 鼠标连点       | packages/burst-engine                           |
| 随机抖动       | packages/burst-engine                           |
| 条件配置集     | apps/main/src-tauri — watcher.rs                |
| 回放速度调节   | apps/main/src-tauri — engine/macro_play.rs      |
| 桌宠扩展动画包 | apps/main — pet/（激活后解锁）                  |

注：鼠标连点当前已作为核心体验开放；v0.5 可按 `MOUSE_BURST` feature bit 收敛为亲友专属或保留开放策略。

---

## 配置文件格式（`.qzh`）

### 二进制结构

```
┌────────────────────────────────────────┐
│ Magic       4 bytes   "QZHU"           │
│ Version     1 byte    0x01             │
│ Flags       2 bytes   reserved         │
│ Nonce      12 bytes   随机（每次写入）  │
├────────────────────────────────────────┤
│ Ciphertext  N bytes   AES-256-GCM 密文  │
├────────────────────────────────────────┤
│ Auth Tag   16 bytes   GCM 认证标签      │
└────────────────────────────────────────┘
```

- AAD：`magic + version + flags`，文件头篡改即验证失败
- 密钥派生：HKDF-SHA256，输入为内嵌 32 字节应用常量
- 宏序列文件复用同一格式，schema 由 `macro_seq.rs` 定义

### JSON Schema 版本兼容（策略 B）

解密后的 JSON 首字段为 `schema_version`，驱动迁移逻辑：

```json
{
  "schema_version": 1,
  "meta": { ... },
  "rules": [ ... ],
  "hotkeys": { ... },
  "advanced": { ... }
}
```

**读取流程：**

```
解密 → 解析 schema_version
  → version == CURRENT  → 直接反序列化
  → version < CURRENT   → 按序执行迁移函数链 migrate_v1→v2 → migrate_v2→v3 → ...
                          → 迁移完成后以新 schema 写回文件
  → version > CURRENT   → 拒绝加载，提示"请升级应用版本"
```

**迁移函数规范（`qzh-profile/src/schema_migrate.rs`）：**

```rust
// 每个版本对应一个迁移函数，接收旧 JSON Value，返回新 JSON Value
fn migrate_v1_to_v2(old: serde_json::Value) -> serde_json::Value { ... }
fn migrate_v2_to_v3(old: serde_json::Value) -> serde_json::Value { ... }

// 迁移链：自动按版本号顺序串联执行
pub fn migrate(mut data: Value, from: u32, to: u32) -> Result<Value> {
    for v in from..to {
        data = match v {
            1 => migrate_v1_to_v2(data),
            2 => migrate_v2_to_v3(data),
            _ => return Err(MigrateError::UnknownVersion(v)),
        };
    }
    Ok(data)
}
```

**新增字段原则：**

- 新字段一律加 `#[serde(default)]`，旧文件缺失字段时填充默认值
- 仅当字段**重命名、移动、删除或类型变更**时才递增 `schema_version`
- 每次递增必须同步编写对应迁移函数，不允许跳版本

**schema 版本进度（持续追加）：**

| schema_version | 变更内容                                              | 引入版本 |
| -------------- | ----------------------------------------------------- | -------- |
| 1              | 初始 schema                                           | v0.1     |
| 2              | 按键字段 `u32` VK → `KeyId`（键盘 + 鼠标 5 键统一）   | v0.2     |
| 3              | `MouseButton` 新增 `WheelUp` / `WheelDown`，支持滚轮连发 | v0.2.4 |
| 4              | `BurstRule` 新增可选 `group` 字段（Toggle 互斥分组），旧文件向后兼容 | v0.2.5 |

---

## 许可证系统（Ed25519 离线校验）

payload：`version u8` / `issue_time u64`（防时钟回拨下界校验）/ `expiry u64` / `features u32`

兑换码格式：`QZHUA-XXXXX-XXXXX-XXXXX-XXXXX`（Base32，Ed25519 签名 + payload）

密钥策略：Ed25519 私钥寄存在 GitHub Secrets，仅在签发 / 构建流程中注入；私钥不进入主应用二进制。主应用只内置由构建注入的校验公钥。

---

## 迭代计划

---

### v0.1｜MVP — 核心连发，快速上线

**目标：** 能用，能更新，能收到用户反馈。

**基础设施（一次性）**

- [x] Monorepo 初始化：根 `Cargo.toml`、`pnpm-workspace.yaml`、`packages/` 骨架
- [x] `apps/main` Tauri v2 + Vite 单页（panel），`tauri.conf.json` 单窗口配置
- [x] `packages/crypto` 骨架（AES-256-GCM + HKDF + Ed25519 校验）
- [x] `packages/migrate` 骨架（`run_migrations()` 泛型函数）
- [x] `packages/qzh-format` 骨架（`FileHeader` + `read/write_encrypted`）+ `packages/qzh-profile`（`Profile` / `BurstRule` 数据结构 + `schema_migrate.rs`）
- [x] GitHub Actions `release.yml`：推 tag 触发 Windows x64 构建 → 签名 → 创建 Draft Release
- [x] GitHub Secrets：`TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 已配置
- [x] `tauri.conf.json` updater endpoint 指向 GitHub Releases `latest.json`
- [x] `tauri-plugin-single-instance`（防重复启动）
- [x] oxlint + oxfmt 集成，git hooks（fmt / clippy / lint）

**连发引擎**

- [x] `windows_sys` `WH_KEYBOARD_LL` / `WH_MOUSE_LL` 全局键鼠监听 + `SendInput` / Interception / DD-HID 三通道模拟
- [x] `SIM_MARKER` + `PENDING_INJECTIONS` 过滤模拟事件循环，`pressed_keys` 过滤 OS key-repeat
- [x] `catch_unwind` 包裹连发循环，panic 后记录日志并补发释放事件
- [x] 按压连发状态机（持键发送，抬键停止）
- [x] Toggle 连发状态机（热键开关，与 Hold 模式统一由 `burst-engine` 低级 hook 驱动）
- [x] 多规则并行支持

**配置持久化**

- [x] Profile 数据结构读写 `.qzh` 文件（AES-256-GCM 加密）
- [x] `tauri-plugin-store` 存储当前激活的配置文件路径
- [x] 启动时自动加载上次使用的配置

**用户协议**

- [x] 首次启动检查协议状态（store 读取 `agreed` / `agreement_version`）
- [x] 未同意则面板展示协议页，屏蔽其他路由；同意后写入存储
- [x] 协议版本变更时强制重新展示

**基础 UI**

- [x] 规则列表（触发键 → 目标键 + 模式 + 间隔）
- [x] 按键录入组件（监听实际按键输入）
- [x] 新增 / 删除规则
- [x] 全局开关

**系统托盘**

- [x] 托盘图标 + 菜单（菜单文字区分启用/禁用状态；启用/禁用动态图标切换待补充）
- [x] 托盘菜单：全局开关 / 打开面板 / 退出

**自动更新**

- [x] `tauri-plugin-updater` 集成（插件注册完成）
- [x] 启动时静默检查并自动下载新版本，下载完成后弹更新公告弹窗（含 Release 正文）；重启后自动安装
- [x] 用户手动"检查更新"触发同一流程，无更新时提示"已是最新版本"

**发布 v0.1**

---

### v0.2｜体验完善 — 稳定性 + 完整配置管理

**目标：** 补齐日常使用必需的体验细节，能放心推给第一批用户。

**日志与崩溃**

- [x] `tracing` + `tracing-appender`，按天滚动日志（保留 7 天，`cleanup_old_logs` 负责清理）
- [x] `std::panic::set_hook` 捕获崩溃，写独立崩溃日志（`crash-{ts}.log`）
- [x] 前端 JS 错误转发到 Rust logger（`log_from_frontend` command）
- [x] 关于 / 诊断入口可打开日志、数据、安装、驱动目录（`open_app_dir` 白名单）

**设置面板**

- [x] 设置面板：通用 / 热键 / 声音 / 配置文件四个分区
- [x] 通用分区：开机自启、关闭行为
- [x] 热键分区：全局开启 / 停止、面板显隐
- [x] 声音分区：全局开关播报、语句、语音、语速、音调、音量、试听
- [x] 配置文件分区：配置卡片、新建、切换、重命名、删除、导入入口

**完整配置管理**

- [x] 后端命令：`save_profile` / `load_profile` / `list_profiles` / `init_default_profile` / `get_active_profile_path` / `rename_profile` / `delete_profile` / `fork_active_profile`
- [x] 多配置文件 UI：新建 / 切换 / 重命名 / 删除
- [x] 默认配置保护：修改默认配置自动 fork
- [x] 外部配置导入：支持丐帮高手 `config.json` 扫描、预览、导入

**托盘完善**

- [x] 开机自启选项（`tauri-plugin-autostart` 已挂载）
- [x] 托盘菜单：打开面板 / 退出

**面板完善**

- [x] 更新提示弹窗（含更新说明）
- [x] 热键冲突检测与提示
- [x] 规则启用/禁用开关（`BurstRule.enabled` 字段已就位）
- [x] 连发间隔数值输入（1ms–10000ms，默认 10ms）

**连发引擎稳定性**

- [x] 规则热更新：替换规则前停止连发线程并清空 toggle 状态
- [x] 后端入参校验：规则数、间隔范围、DD-HID 特殊约束在命令层 / profile 层双重校验
- [x] 模拟事件过滤：SendInput 使用 `SIM_MARKER`，驱动通道使用 `PENDING_INJECTIONS`
- [x] 多规则隔离：模拟目标键不会触发其他规则的启动 / 停止逻辑
- [x] OS key-repeat 过滤：`pressed_keys: HashSet<KeyId>` 只响应物理首次按下

**输入范围扩展**

- [x] 数据模型：`packages/qzh-profile/src/key_id.rs` 引入 tagged `KeyId = Keyboard(u32) | Mouse(MouseButton)`；`BurstRule.{trigger_key,target_key,stop_key}` 与 `Hotkeys.{global_toggle,global_stop,panel_toggle}` 全部改用 `KeyId`
- [x] schema v1→v2 自动迁移：旧裸 VK 包装为 `{kind:"keyboard",code:VK}`，可选字段 `null` 保留
- [x] 全局物理按键 hook 扩鼠标：与键盘 hook 共用消息循环线程加装 `WH_MOUSE_LL`，识别 5 键 + `WM_XBUTTONDOWN/UP` 高 16 位的 X1/X2，过滤 SIM_MARKER 与自循环
- [x] 三通道注入支持鼠标 5 键：SendInput `INPUT_MOUSE` + `MOUSEEVENTF_*`、DD `DD_btn`（X1/X2 不在值域，自动回退 SendInput + 一次 warn）、Interception `InterceptionMouseStroke` + 鼠标设备扫描
- [x] 前端 KeyCapture 扩约 120 项键盘白名单（F13–F24 / 小键盘 / OEM 标点 / 编辑键 / 修饰键独立位）+ 鼠标 5 键 onMouseDown 录入
- [x] DD-HID schema validate 拦截 `target_key = Mouse(X1|X2)`，UI 提示用户改用 SendInput / Interception 模式

**v0.2 收尾能力**

- [x] 全局热键：配置级开启键 / 停止键 / 面板显隐键，启动期立即生效
- [x] 声音反馈：全局开关切换语音播报，支持系统语音列表兼容回退
- [x] 诊断修复：管理员权限、开机自启、输入模式、驱动状态、安装前置检查、DD-HID / Interception 残留修复
- [x] 驱动资源完整性校验与 DD-HID 诊断报告导出
- [x] WebView 聚焦场景热键中继与默认快捷键拦截
- [x] schema v2→v3：新增滚轮上 / 下，SendInput、Interception、DD-HID 三通道支持滚轮注入
- [x] 面板显隐热键使用最小化 / 恢复语义；托盘打开、托盘双击、再次启动统一唤回面板

**互斥分组与规则编排（0.2.5–0.2.7）**

- [x] schema v3→v4：`BurstRule` 新增可选 `group` 字段；Toggle 规则同组互斥，激活一条自动停止同组其他活跃规则
- [x] 互斥分组语音播报修正：同组切换时只播报「新规则开始」，不误播「停止 N」
- [x] 分组容器折叠 / 展开（chevron 指示）、分组标题铅笔图标编辑、解散二次确认
- [x] 按压连发规则支持拖拽排序
- [x] 配置卡片简略信息展示互斥组数量（有分组时）

**输入后端迭代（0.2.5–0.2.7）**

- [x] DDHID 临时屏蔽：加载旧配置或状态同步命中 DDHID 自动回退通用模式并提示；用户主动切换被阻止；诊断修复保留卸载入口
- [x] 恢复 DD驱动通道（DdSimple，基于 `dd63330.dll`）：独立于 DDHID 安装链路，支持键盘 / 鼠标 / 滚轮 / 侧键注入，非管理员运行时引导提权重启
- [x] DD驱动滚轮 / 侧键映射修正：滚轮用 DD SDK 上滚 / 下滚编码，侧键按 `MOUSE_INPUT_DATA.ButtonFlags` 发 X1 / X2
- [x] `dd63330.dll` 纳入打包资源、运行时完整性校验与发版前资源检查
- [x] 禁止 DD 系列（DdSimple / DdHid）同键 Toggle（无法过滤自身注入）
- [x] 用户协议升级至 v1.3（补充 DD驱动模式、DDHID 暂停、驱动残留与卸载说明）

**界面优化（0.2.5–0.2.7）**

- [x] 全局关闭态面板背景改为品牌色去饱和薰衣草渐变；规则激活脉冲动画加速
- [x] 全局统一 3px 细滚动条（全局开启态切换为白色半透明）
- [x] 纯告知型弹窗合并为单「知道了」按钮

**发布 v0.2**

---

### v0.3｜体验与稳定性收尾

**目标：** 不开新大模块，补齐 v0.2 遗留的体验细节与稳健性收尾，巩固第一批用户口碑。以 0.2.x / 0.3.x 补丁形式滚动发布。

**崩溃与诊断**

- [ ] 崩溃提示窗口（打开日志文件夹 / 复制路径 / 关闭）— 当前仅写日志，未弹窗
- [ ] 托盘启用 / 禁用动态图标切换（当前仅菜单文字区分状态）

**首次引导**

- [ ] 协议同意后展示简短引导流程（步骤提示 + 创建第一条规则向导）
- [ ] 内置常用规则模板（FPS 快速连发、MOBA 技能连按等），一键导入当前配置

**配置管理收尾**

- [ ] 导入 / 导出原生 `.qzh` 文件（跨设备迁移、社区分享）
- [ ] `notify` 监听配置文件变更自动 reload；失效时定时轮询兜底
- [ ] 托盘菜单：切换配置文件（动态菜单项）

**交互优化**

- [ ] 连发间隔滑块 / 步进器优化（数值输入之外的快速调节）

**发版应急与回退（预案 + 护栏，文档级先行）**

> 前提：Tauri updater 只升不降——已升级用户无法被「降级」拉回，紧急回退实质是「向前滚一个修复版」。详见下文路径。

- [x] 应急回退 runbook（`docs/RELEASE_ROLLBACK.md`）：含两条路径的真实命令——
  - ① **止血（分钟级）**：GitHub 把坏 Release 转 Draft / 把上个好版本「Set as latest」，使 `latest.json` 不再分发坏版本，拦住尚未升级的用户（已静默下载暂存 `pending_update` 的无法召回）
  - ② **向前滚修复**：`git revert` 坏改动 → bump 更高 patch 号 → 打 tag → CI 出包 → 自动更新带走已中招用户（修复已升级用户的唯一路径）
- [ ] 发版护栏写入 `CLAUDE.md` 发版流程：高风险版本**禁止裸 bump `CURRENT_SCHEMA_VERSION`**（新字段一律 `#[serde(default)]` 走兼容；否则回退到旧 schema 会 `TooNew` 拒载、砸用户配置）；风险改动尽量挂运行时开关，先关开关而非全量回退
- [ ] 保留上个稳定版安装包 + `.sig`（旧 GitHub Release 不删除），确保向前滚 / 手动回退有可用且已签名的产物

**发布 v0.3**

---

### v0.4｜桌宠模式

**目标：** 上线桌宠，提升产品差异化和趣味性。

- [ ] `tauri.conf.json` 新增 pet 窗口（transparent / decorations:false / alwaysOnTop）
- [ ] Vite 新增 `pet.html` 入口
- [ ] `Window::set_ignore_cursor_events()` 点击穿透控制
- [ ] 鼠标坐标检测，hover 时关闭穿透，离开恢复
- [ ] 拖拽移动 + 位置持久化
- [ ] 桌宠前端：SVG 角色 + CSS 动画（Idle / Burst / Hover）
- [ ] `useEngineStatus` hook（复用 `global-enabled-changed` / `app-status-changed`，并为激活规则补统一事件）
- [ ] `usePetAnim` 动画状态机
- [ ] 右键菜单（开关 / 打开面板 / 退出）
- [ ] 左键状态气泡（3 秒淡出）
- [ ] 托盘菜单新增：打开/关闭桌宠
- [ ] 补充 Alert / Sleep 动画状态

**发布 v0.4**

---

### v0.5｜许可证系统 + 亲友专属功能

**目标：** 上线付费通道和高价值功能，开始商业化。

**许可证**

- [ ] `apps/keygen` CLI：生成 Ed25519 密钥对，签名输出兑换码
- [ ] GitHub Secrets 注入 Ed25519 签发 / 构建流程（私钥不进主应用二进制）
- [ ] 激活面板 UI：输入兑换码、显示到期时间和已激活功能
- [ ] 引擎启动时读取激活记录，按 feature bits 控制功能开关
- [ ] 到期前 7 天 UI 提醒（面板 banner + 桌宠状态气泡）
- [ ] 到期后亲友专属功能自动降级（不崩溃，不锁死）
- [ ] 许可证状态面板：剩余天数、激活时间、已授权功能列表

**亲友专属功能**

- [ ] 鼠标连点限制策略（当前开放，v0.5 决定是否按 `MOUSE_BURST` 收敛）
- [ ] 随机抖动（间隔 ± 可配置随机偏差）
- [ ] 宏录制（事件流 + 时间戳，存为 `.qzh`）
- [ ] 宏回放（原速 / 倍速）+ 热键绑定

**发布 v0.5**

---

### v0.6｜落地页 + 运营基础

**目标：** 有对外展示的门面，支撑用户增长。

- [ ] `apps/release-server` Axum 服务，`rust-embed` 内嵌静态资源
- [ ] 落地页 `/`（介绍 + 截图 + 下载按钮 → GitHub Releases）
- [ ] 下载页 `/download`（平台 + 版本信息）
- [ ] 更新日志页 `/changelog`（读取 / 转换 `CHANGELOG.md`，避免维护第二份内容源）
- [ ] 健康检查 `/health`
- [ ] 桌宠激活后解锁扩展动画状态
- [ ] 更新分发降险（缩小坏版本「即时全量铺开」的爆炸半径，让回退预案更从容）：
  - [ ] min-version / kill-switch：远端清单可声明「最低可用版本」或强制下线某版本，客户端启动时校验
  - [ ] 分批灰度发布：按比例 / 分批放量，先小范围验证再全量
  - [ ] 静默更新延迟自动安装：暂存 `pending_update` 后延迟 N 小时再装，留出发现问题与止血的窗口

**发布 v0.6**

---

### v1.0｜完整功能

- [ ] 条件配置集（检测活动进程，自动切换配置文件）
- [ ] 回放速度调节 UI（0.5x / 1x / 2x）
- [ ] 桌宠扩展动画包（付费解锁）
- [ ] Mac 兼容（辅助功能权限引导，评估 macOS 原生监听 / 注入实现）
- [ ] Azure Trusted Signing 代码签名（GitHub Actions 集成，每次 release 自动签名）；早期版本在安装说明中注明 SmartScreen 绕过方式（"更多信息 → 仍要运行"）

---

### 待定（最低优先级）

- [ ] 桌宠输入响应模式（`gilrs` 手柄 + 键鼠动画反馈，参考 Dongocat）
- [ ] 多平台发布与下载页支持（darwin-aarch64、linux-x86_64）

---

## 技术选型

| 用途              | 库 / 工具                                                  |
| ----------------- | ---------------------------------------------------------- |
| 全局键盘监听      | `windows_sys` `WH_KEYBOARD_LL`（Windows）；macOS 待定      |
| 全局鼠标监听      | `windows_sys` `WH_MOUSE_LL`（Windows，含 X1/X2 侧键）；macOS 待定 |
| 按键模拟          | `windows_sys` `SendInput` + `KEYEVENTF_SCANCODE`（Windows）；macOS 待定 |
| 鼠标按钮模拟      | `windows_sys` `SendInput INPUT_MOUSE` + `MOUSEEVENTF_*`（Windows） |
| 全局热键监听      | `burst-engine` 共用 `WH_KEYBOARD_LL` / `WH_MOUSE_LL`，热键优先于规则处理 |
| 点击穿透          | `Window::set_ignore_cursor_events()`（Tauri 内置，跨平台） |
| 自动升级          | `tauri-plugin-updater`                                     |
| 配置文件加密      | `aes-gcm`                                                  |
| 密钥派生          | `hkdf` + `sha2`                                            |
| 许可证签名校验    | `ed25519-dalek`                                            |
| 兑换码编解码      | `base32`                                                   |
| 配置文件变更监听  | `notify`（规划，当前未接入）                              |
| 手柄输入监听      | `gilrs`（待定低优先级）                                    |
| HTTP 更新服务     | GitHub Releases + Tauri updater；独立 `axum` 落地页待定    |
| 应用状态持久化    | `tauri-plugin-store`                                       |
| 开机自启          | `tauri-plugin-autostart`                                   |
| 前端动画          | CSS 关键帧 + SVG（MVP）/ Lottie（后期）                    |
| 前端 UI           | React + TypeScript + 原生 CSS                              |
| 前端构建          | Vite（当前 `panel.html` 单入口，`pet.html` 待 v0.4）       |
| Monorepo 管理     | Cargo workspace + pnpm workspaces                          |

---

## 风险与缓解（已确认）

| #   | 风险                       | 缓解方案                                                            | 状态 |
| --- | -------------------------- | ------------------------------------------------------------------- | ---- |
| ①   | 低级 hook 与模拟输入自循环 | `SIM_MARKER` + `PENDING_INJECTIONS` 过滤模拟事件                    | 消除 |
| ②   | 反作弊软件拦截模拟输入     | 不做技术规避，EULA + 文档明确说明                                   | 确认 |
| ③   | 桌宠被全屏游戏覆盖         | 桌宠未实现；实现时文档 QA 告知，建议边框全屏                       | 待评估 |
| ④   | 点击穿透时无法触发右键菜单 | 桌宠未实现；规划动态调用 `set_ignore_cursor_events()`               | 待评估 |
| ⑤   | AES 密钥被逆向提取         | 接受"防普通用户"定位，不对抗专业逆向                                | 确认 |
| ⑥   | 系统时间回拨绕过许可证     | payload 含 issue_time 做下界校验；`last_verified_at` 列为扩展优化点 | 确认 |
| ⑦   | 多窗口 IPC 复杂度          | 当前只有面板窗口；后续桌宠继续使用 Tauri event，不引入 Named Pipe    | 消除 |
| ⑧   | 更新包被中间人替换         | Tauri updater 强制 .sig 签名验证 + HTTPS                            | 确认 |
| ⑨   | Mac 点击穿透 API 不同      | 改用 Tauri 内置 `set_ignore_cursor_events()`，跨平台，风险消除      | 消除 |
| ⑩   | SIM_COUNT 竞态（非 Unicode 键） | 已用 `windows_sys` + `SIM_MARKER` 精确标记，hook 统一过滤，无竞态 | 消除 |
| ⑪   | 模拟 KeyRelease 未过滤 | 同上，所有 SendInput（含 key_up）均带 SIM_MARKER，hook 不再区分 press/release 统一跳过 | 消除 |
| ⑫   | DD-HID / Interception 驱动残留 | 诊断修复提供安装前置检查、残留识别、深度清理与重启提示 | 缓解 |
| ⑬   | WebView 聚焦吞掉热键 / 默认快捷键抢占 | 前端中继键盘事件到后端，并阻止非编辑区默认快捷键 | 缓解 |
| ⑭   | DDHID 驱动在部分环境不稳定 | 临时屏蔽 DDHID：命中自动回退通用模式并提示，阻止主动切换，保留卸载入口；改用 DD驱动（DdSimple）覆盖同类场景 | 缓解 |
| ⑮   | 坏版本经静默自动更新即时全量铺开，且 Tauri updater 不可降级 | v0.3 应急回退 runbook（转 Draft 止血 + 向前滚修复）+ 发版护栏（高风险版本禁止裸 bump schema，防回退砸配置）；v0.6 评估 kill-switch / 灰度 / 延迟自动安装 | 规划 |
