# 气质花 — Tauri 按键助手 路线图

## 项目概述

面向游戏辅助的按键助手。基础功能免费开放，高级功能通过兑换码离线激活使用时长。
Monorepo 结构，Rust workspace + pnpm workspace 双层管理。
支持三种运行模式：面板模式、托盘模式、桌宠模式，均在单一 Tauri 进程内管理。

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

| 场景 | 容错策略 |
|------|---------|
| 连发引擎线程 panic | `catch_unwind` 捕获，记录日志，自动重启引擎线程，托盘图标短暂提示 |
| 配置文件读取失败（损坏/篡改） | 提示用户，提供"恢复默认配置"选项，不阻塞启动 |
| `notify` 文件监听失效 | 定时轮询兜底（每 30 秒检查一次配置变更） |
| 自动升级下载失败 | 最多重试 3 次，失败后静默跳过，不影响正常使用 |
| `rdev` 监听初始化失败 | 提示"全局监听启动失败，请以管理员权限运行"，其余功能保持可用 |
| 规则热键冲突 | 检测到冲突时弹提示，保留旧绑定，新绑定不生效，不崩溃 |
| Tauri Command 异常 | 所有 Command 返回 `Result`，前端统一错误处理，显示 toast 提示 |

**引擎线程隔离：** 连发引擎在独立线程运行，panic 不传播到主进程，`std::panic::catch_unwind` 包裹主循环。

### 四、日志完善，崩溃可追溯

**目标：** 出问题时用户能一键提供有效日志，开发者能快速定位问题。

#### 日志系统

- **Rust 端**：`tracing` + `tracing-subscriber` + `tracing-appender`
  - 按天滚动日志文件，保留最近 7 天
  - 日志路径：`%AppData%\qzhua\logs\qzhua-YYYY-MM-DD.log`
  - 级别：`ERROR` / `WARN` / `INFO` / `DEBUG`（可在设置中切换，默认 INFO）
- **前端端**：JS 错误通过 Tauri Command 转发到 Rust logger，统一写入同一日志文件
- **结构化格式**：每条日志含时间戳、级别、模块路径、线程 ID

#### 崩溃处理

```
std::panic::set_hook → 捕获 panic 信息
  → 写入 crash-YYYY-MM-DD-HHmmss.log（独立崩溃日志）
  → 弹出崩溃提示窗口（Tauri 原生对话框）
      ┌─────────────────────────────────────┐
      │ 气质花遇到了一个问题并已崩溃          │
      │                                     │
      │ 崩溃日志已保存，如需报告问题请提供：  │
      │ C:\Users\...\qzhua\logs\crash-...   │
      │                                     │
      │ [打开日志文件夹]  [复制日志路径]      │
      │              [确定关闭]              │
      └─────────────────────────────────────┘
```

#### 用户反馈引导

- 设置面板提供"查看日志文件夹"入口（日常排查用）
- 崩溃弹窗提供"复制日志路径"按钮（一键复制，方便粘贴给开发者）
- 日志文件夹直接用系统文件管理器打开，降低操作门槛
- 日志文件不含任何用户个人信息（无硬件 ID、无用户名），用户无需顾虑隐私

#### 日志分级规范

| 级别 | 使用场景 |
|------|---------|
| ERROR | 功能不可用的严重错误（引擎崩溃、文件损坏） |
| WARN | 降级运行的异常（重试成功、配置回退） |
| INFO | 关键状态变更（连发启动/停止、配置切换、许可证激活） |
| DEBUG | 详细运行信息（每次按键事件、IPC 消息），默认关闭 |

---

## 应用模式

### 面板模式（Panel）
全功能配置界面，适合初始设置和规则管理。常规窗口，可最小化到托盘。

### 托盘模式（Tray）
关闭面板和桌宠窗口后，仅系统托盘图标常驻。极低资源占用，适合日常游戏中后台运行。
托盘菜单提供开关、切换模式、退出等快捷操作。

### 桌宠模式（Pet）
透明无边框窗口，始终置顶。角色浮在桌面或游戏画面角落，通过动画反映当前连发状态。
默认点击穿透（游戏中不影响操作），鼠标悬停时临时关闭穿透以支持拖拽和右键菜单。

---

## Monorepo 目录结构

```
气质花/
├── Cargo.toml                          # Rust workspace 根
├── pnpm-workspace.yaml                 # pnpm workspace
├── package.json                        # 根脚本（dev / build / release）
│
├── apps/
│   ├── main/                           # 单一 Tauri 应用（面板 + 桌宠 + 托盘）
│   │   ├── package.json
│   │   ├── panel.html                  # 面板窗口入口
│   │   ├── pet.html                    # 桌宠窗口入口
│   │   ├── src/
│   │   │   ├── windows/
│   │   │   │   ├── panel/              # 面板窗口 UI
│   │   │   │   │   ├── components/
│   │   │   │   │   │   ├── RuleList/
│   │   │   │   │   │   ├── KeyInput/
│   │   │   │   │   │   ├── ProfilePanel/
│   │   │   │   │   │   ├── LicensePanel/
│   │   │   │   │   │   └── UpdatePanel/
│   │   │   │   │   ├── pages/
│   │   │   │   │   │   └── AgreementPage/
│   │   │   │   │   └── PanelApp.tsx
│   │   │   │   └── pet/                # 桌宠窗口 UI
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
│   │       ├── Cargo.toml              # 依赖 qzh-format、crypto
│   │       ├── tauri.conf.json         # 多窗口配置（panel + pet）
│   │       └── src/
│   │           ├── main.rs
│   │           ├── engine/             # 连发引擎
│   │           │   ├── burst.rs        # 按压连发 + Toggle 连发
│   │           │   ├── macro_play.rs   # 宏回放
│   │           │   └── input.rs        # rdev 监听 + enigo 模拟
│   │           ├── tray.rs             # 系统托盘图标与菜单
│   │           ├── watcher.rs          # 配置文件变更监听（notify）
│   │           └── commands/
│   │               ├── profile.rs      # 配置文件 CRUD、导入导出
│   │               ├── license.rs      # 兑换码激活与查询
│   │               ├── engine.rs       # 引擎控制（开关、规则热更新）
│   │               └── updater.rs      # 检查更新、下载安装
│   │
│   ├── keygen/                         # 兑换码生成 CLI（纯 Rust，不打包进应用）
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   │
│   └── release-server/                 # 落地页服务（Axum，仅展示，文件托管在 GitHub）
│       ├── Cargo.toml
│       ├── config.toml                 # 端口、GitHub repo 信息、站点基础信息
│       ├── content/
│       │   └── changelog.toml          # 更新日志数据（手动维护，每次发布追加）
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
│           └── changelog.rs            # 解析 changelog.toml，渲染日志数据
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
    └── qzh-format/                     # .qzh 文件格式（Rust lib crate）
        ├── Cargo.toml
        └── src/
            ├── lib.rs
            ├── header.rs
            ├── profile.rs
            ├── macro_seq.rs
            └── migrate.rs              # schema 版本迁移链（依赖 packages/migrate）
```

---

## 架构说明

### 进程模型

```
┌──────────────────────────────────────────────────────────┐
│  qzhua.exe（单一 Tauri 进程）                             │
│                                                          │
│  ┌─────────────────┐   Tauri emit_all()  ┌────────────┐ │
│  │  Rust 后端       │ ──────────────────→ │ 面板窗口   │ │
│  │  - 连发引擎      │                     │ WebView    │ │
│  │  - 系统托盘      │ ──────────────────→ │ 桌宠窗口   │ │
│  │  - 配置监听      │                     │ WebView    │ │
│  │  - 许可证校验    │                     └────────────┘ │
│  └─────────────────┘                                     │
└──────────────────────────────────────────────────────────┘
               │ HTTPS
       ┌───────┴────────┐
       │ release-server │
       └────────────────┘
```

### 发布基础设施

| 职责 | 平台 | 说明 |
|------|------|------|
| 安装包托管 | GitHub Releases | `.exe` / `.nsis.zip` + `.sig` 签名文件 |
| Tauri updater 端点 | GitHub Releases | 每次发布上传 `latest.json`（updater manifest） |
| 私钥存储 | GitHub Actions Secrets | Tauri 签名私钥、Ed25519 许可证私钥 |
| CI/CD 构建 | GitHub Actions | 推 tag 触发自动构建、签名、发布 |
| 落地页 | release-server（自托管） | 仅渲染 HTML，下载链接指向 GitHub Releases |

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

每次 GitHub Actions 发布时自动生成并上传 `latest.json`（Tauri 标准 updater manifest 格式）。

### GitHub Actions 发布流程

```
推送 tag（v1.2.0）
  → actions/checkout
  → 矩阵构建（matrix.os: [windows-latest, macos-latest]）
      ├── 各平台分别安装 Rust + Node.js + pnpm
      ├── pnpm install && pnpm build
      └── cargo tauri build（从 Secrets 注入 Tauri 签名私钥）
  → 合并产物，生成 latest.json（updater manifest）
  → 创建 GitHub Release，上传：
      ├── qzhua_1.2.0_x64-setup.exe          # Windows
      ├── qzhua_1.2.0_x64-setup.nsis.zip
      ├── qzhua_1.2.0_x64-setup.nsis.zip.sig
      ├── qzhua_1.2.0_x64.dmg                # macOS（v1.0 后）
      └── latest.json
  → 触发 release-server 落地页更新（可选 webhook）

注：Tauri 不支持交叉编译，各平台必须在对应 runner 上构建。仓库设为 public 享受无限 Actions 分钟数。
```

### release-server 路由（落地页服务）

| 路由 | 用途 |
|------|------|
| `GET /` | 落地页（介绍 + 功能 + 截图 + 下载按钮 → GitHub Releases） |
| `GET /download` | 下载页（最新版本信息，链接指向 GitHub Releases） |
| `GET /changelog` | 更新日志页（读取 changelog.toml） |
| `GET /health` | 健康检查 |

### changelog.toml 数据格式

```toml
[[versions]]
version = "1.2.0"
date = "2025-06-01"
title = "修复与优化"
notes = [
    "修复连发引擎在部分游戏中失效的问题",
    "优化桌宠动画流畅度",
]

[[versions]]
version = "1.1.0"
date = "2025-05-01"
title = "新增功能"
notes = [
    "新增随机抖动功能（高级功能）",
    "新增宏录制与回放",
]
```

每次发布时追加一条记录，`GET /changelog` 倒序渲染。

---

**单进程多窗口**：面板和桌宠是同一 Tauri 应用的两个独立 WebView 窗口，各自有独立的 HTML 入口（`panel.html` / `pet.html`），通过 Tauri 内置事件系统通信。

**窗口配置（tauri.conf.json）：**

```json
{
  "windows": [
    {
      "label": "panel",
      "url": "panel.html",
      "visible": false,
      "width": 900,
      "height": 600
    },
    {
      "label": "pet",
      "url": "pet.html",
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
  → app.emit_all("engine_state", payload)
  → 面板窗口更新状态显示
  → 桌宠窗口驱动动画状态机切换
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

apps/release-server（仅依赖 axum / tokio / rust-embed）
```

### 数据存储路径约定

| 数据类型 | 路径 | 说明 |
|----------|------|------|
| 配置文件（`.qzh`） | `{app_data_dir}/profiles/` | 用户导入/导出通过文件对话框，不暴露内部路径 |
| 应用设置 | `{app_data_dir}/settings.json` | `tauri-plugin-store` 管理，`#[serde(default)]` + `packages/migrate` 迁移链 |
| 日志 | `{app_log_dir}/` | `tracing-appender` 按天滚动，保留 7 天 |
| 崩溃日志 | `{app_log_dir}/crash-YYYY-MM-DD-HHmmss.log` | panic hook 写入 |

`app_data_dir` / `app_log_dir` 由 Tauri `PathResolver` 跨平台解析，Windows 对应 `%APPDATA%\qzhua`，macOS 对应 `~/Library/Application Support/qzhua`。

### 输入参数约束

| 参数 | 有效范围 | 说明 |
|------|----------|------|
| 连发间隔 | 10ms – 10000ms | UI 做 clamp，低于 10ms 无实际意义且可能触发系统限流 |
| 单配置规则数 | ≤ 64 条 | 线性匹配不成瓶颈，上限防止滥用 |
| 宏序列步骤数 | ≤ 256 步 | 防止回放时间过长 |

约束在 `qzh-format` 的 schema 校验层执行（clamp 或 reject），不依赖前端验证。

### 卸载清理策略

| 数据 | 策略 | 说明 |
|------|------|------|
| 用户配置（`.qzh`） | 保留 | 符合 Windows 惯例，重装后数据仍在 |
| 应用设置（`settings.json`） | 保留 | 同上 |
| 日志文件 | 保留 | 用户可手动清理 |
| 注册表自启动项 | 由 installer 清除 | NSIS/MSI 卸载脚本负责移除 `tauri-plugin-autostart` 写入的注册表项 |
| v1.0 可选 | 卸载时询问是否同时删除配置数据 | 不强制，给用户选择 |

---

## 桌宠模式设计

### 点击穿透

使用 Tauri 内置 `Window::set_ignore_cursor_events(bool)`（跨平台，Windows / macOS 均支持）。

rdev 全局鼠标监听计算光标是否进入桌宠窗口区域：
- 进入 → `set_ignore_cursor_events(false)` 关闭穿透（支持拖拽和右键）
- 离开 → `set_ignore_cursor_events(true)` 恢复穿透

### 动画状态机

| 状态 | 动画描述 | 触发条件 |
|------|---------|---------|
| Idle | 缓慢呼吸，偶尔眨眼 | 默认 |
| Burst | 快速抖动或奔跑循环 | 连发引擎激活 |
| Hover | 抬头看向光标，尾巴摇动 | 鼠标进入窗口区域 |
| Alert | 耳朵竖起，眼睛放大 | 切换配置文件 |
| Sleep | 闭眼 ZZZ | 空闲超过 N 分钟（可配置） |

### 交互行为

- **拖拽**：关闭穿透后鼠标按下拖动，位置存入 `tauri-plugin-store`，重启后恢复
- **右键菜单**：开关连发 / 切换配置文件 / 打开面板 / 退出
- **左键单击**：显示状态气泡（当前规则、许可证到期），3 秒淡出

### 动画资源

MVP 阶段用 CSS 动画 + SVG，后期视美术资源情况升级为 Sprite Sheet 或 Lottie。

### 扩展模式：输入响应动画（最低优先级）

参考 Dongocat，监听全局键盘、鼠标、手柄输入做出对应动画反馈，与连发引擎解耦。

| 状态 | 触发条件 |
|------|---------|
| Typing | 连续键盘输入（>2次/秒） |
| KeyPress | 单次按键 |
| MouseMove | 鼠标移动（眼睛跟随） |
| Click | 鼠标点击（眨眼） |
| GamepadButton | 手柄按键 |
| GamepadStick | 摇杆偏移（身体倾斜） |

手柄监听通过 `gilrs` crate 实现，阶段三末尾实现，需配套美术资源。

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
  "agreement_version": "1.0",
  "app_version_at_agree": "1.0.0"
}
```

### 协议核心条款

| 条款 | 内容 |
|------|------|
| 使用风险 | 模拟输入可能触发游戏反作弊机制，导致账号封禁，用户自行承担全部风险 |
| 适用范围 | 仅供个人娱乐与技术学习使用 |
| 禁止商用 | 严禁用于任何商业目的 |
| 免责声明 | 因使用本软件导致的任何损失，开发者不承担责任 |
| 知识产权 | 未经授权不得反编译、修改或二次分发 |

协议正文滚动到底部才激活同意按钮。`agreement_version` 与代码硬编码版本不一致时强制重新同意。

---

## 功能分层

### 免费功能
| 功能 | 实现位置 |
|------|---------|
| 用户协议（首次启动） | apps/main — AgreementPage |
| 按压连发 | apps/main/src-tauri — engine/burst.rs |
| 一键连发（热键 Toggle） | apps/main/src-tauri — engine/burst.rs |
| 配置文件 CRUD + 导入导出 | apps/main — commands/profile.rs |
| `.qzh` 加密格式 | packages/qzh-format |
| 快速切换配置文件 | 面板 UI + 托盘菜单 |
| 系统托盘 & 开机自启 | apps/main/src-tauri — tray.rs |
| 桌宠模式（基础动画） | apps/main — pet/PetApp.tsx |
| 自动升级 | apps/main — commands/updater.rs |

### 高级功能（兑换码激活，限时）
| 功能 | 实现位置 |
|------|---------|
| 宏录制与回放 | apps/main/src-tauri — engine/macro_play.rs |
| 鼠标连点 | apps/main/src-tauri — engine/burst.rs |
| 随机抖动 | apps/main/src-tauri — engine/burst.rs |
| 条件配置集 | apps/main/src-tauri — watcher.rs |
| 回放速度调节 | apps/main/src-tauri — engine/macro_play.rs |
| 桌宠扩展动画包 | apps/main — pet/（激活后解锁） |

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

**迁移函数规范（`qzh-format/src/migrate.rs`）：**

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

**版本历史（持续追加）：**

| schema_version | 变更内容 | 引入版本 |
|---------------|---------|---------|
| 1 | 初始 schema | v0.1 |

---

## 许可证系统（Ed25519 离线校验）

payload：`version u8` / `issue_time u64`（防时钟回拨下界校验）/ `expiry u64` / `features u32`

兑换码格式：`QZHUA-XXXXX-XXXXX-XXXXX-XXXXX`（Base32，Ed25519 签名 + payload）

---

## 迭代计划

---

### v0.1｜MVP — 核心连发，快速上线

**目标：** 能用，能更新，能收到用户反馈。

**基础设施（一次性）**
- [ ] Monorepo 初始化：根 `Cargo.toml`、`pnpm-workspace.yaml`、`packages/` 骨架
- [ ] `apps/main` create-tauri-app，Vite 单页（仅 panel），`tauri.conf.json` 单窗口配置
- [ ] `packages/crypto` 骨架（AES-256-GCM + HKDF，供 qzh-format 使用）
- [ ] `packages/migrate` 骨架（`VersionedData` trait + `migrate()` 泛型函数）
- [ ] `packages/qzh-format` 骨架（Profile 数据结构 + 加密读写 + `schema_version` 字段 + `migrate.rs` 迁移框架，依赖 `packages/migrate`）
- [ ] GitHub Actions `release.yml`：推 tag 触发矩阵构建（`windows-latest` / `macos-latest`）→ 签名 → 发布 GitHub Release
- [ ] GitHub Secrets：`TAURI_SIGNING_PRIVATE_KEY`
- [ ] `tauri.conf.json` updater endpoint 指向 GitHub Releases `latest.json`
- [ ] `tauri-plugin-single-instance`（防重复启动）

**连发引擎**
- [ ] `rdev` 全局键盘监听 + `enigo` 按键模拟
- [ ] `AtomicUsize` sim_count 过滤事件循环
- [ ] `catch_unwind` 包裹引擎主循环，panic 后自动重启
- [ ] 按压连发状态机（持键发送，抬键停止）
- [ ] Toggle 连发状态机（热键开关，`tauri-plugin-global-shortcut`）
- [ ] 多规则并行支持

**配置持久化**
- [ ] Profile 数据结构读写 `.qzh` 文件（AES-256-GCM 加密）
- [ ] `tauri-plugin-store` 存储当前激活的配置文件路径
- [ ] 启动时自动加载上次使用的配置

**用户协议**
- [ ] 首次启动检查协议状态（store 读取 `agreed` / `agreement_version`）
- [ ] 未同意则面板展示协议页，屏蔽其他路由；同意后写入存储
- [ ] 协议版本变更时强制重新展示

**基础 UI**
- [ ] 规则列表（触发键 → 目标键 + 模式 + 间隔）
- [ ] 按键录入组件（监听实际按键输入）
- [ ] 新增 / 删除规则
- [ ] 全局开关

**系统托盘**
- [ ] 托盘图标（启用/禁用双状态）
- [ ] 托盘菜单：全局开关 / 打开面板 / 退出

**自动更新**
- [ ] `tauri-plugin-updater` 集成
- [ ] 启动时检查更新，有新版本弹提示（版本号 + 确认安装）

**发布 v0.1**

---

### v0.2｜体验完善 — 稳定性 + 完整配置管理

**目标：** 补齐日常使用必需的体验细节，能放心推给第一批用户。

**日志与崩溃**
- [ ] `tracing` + `tracing-appender`，按天滚动日志（保留 7 天）
- [ ] `std::panic::set_hook` 捕获崩溃，写独立崩溃日志
- [ ] 崩溃提示窗口（打开日志文件夹 / 复制路径 / 关闭）
- [ ] 前端 JS 错误转发到 Rust logger
- [ ] 设置页"查看日志文件夹"入口

**设置面板**
- [ ] 设置面板骨架：通用 / 配置文件 / 关于 分区，后续迭代按需填充内容

**首次引导**
- [ ] 协议同意后展示简短引导流程（步骤提示 + 创建第一条规则向导）

**完整配置管理**
- [ ] 多配置文件：新建 / 重命名 / 删除
- [ ] 配置文件下拉快速切换
- [ ] 导入 / 导出 `.qzh` 文件
- [ ] `notify` 监听配置文件变更自动 reload；失效时定时轮询兜底

**托盘完善**
- [ ] 开机自启选项（`tauri-plugin-autostart`）
- [ ] 托盘菜单：切换配置文件 / 打开面板 / 退出

**面板完善**
- [ ] 更新提示弹窗（含更新说明）
- [ ] 热键冲突检测与提示
- [ ] 规则启用/禁用开关（不删除规则）
- [ ] 连发间隔滑块 + 数值输入

**发布 v0.2**

---

### v0.3｜桌宠模式

**目标：** 上线桌宠，提升产品差异化和趣味性。

- [ ] `tauri.conf.json` 新增 pet 窗口（transparent / decorations:false / alwaysOnTop）
- [ ] Vite 新增 `pet.html` 入口
- [ ] `Window::set_ignore_cursor_events()` 点击穿透控制
- [ ] rdev 鼠标坐标检测，hover 时关闭穿透，离开恢复
- [ ] 拖拽移动 + 位置持久化
- [ ] 桌宠前端：SVG 角色 + CSS 动画（Idle / Burst / Hover）
- [ ] `useEngineStatus` hook（监听 Tauri `engine_state` 事件）
- [ ] `usePetAnim` 动画状态机
- [ ] 右键菜单（开关 / 打开面板 / 退出）
- [ ] 左键状态气泡（3 秒淡出）
- [ ] 托盘菜单新增：打开/关闭桌宠
- [ ] 补充 Alert / Sleep 动画状态

**发布 v0.3**

---

### v0.4｜许可证系统 + 高级功能

**目标：** 上线付费通道和高价值功能，开始商业化。

**许可证**
- [ ] `apps/keygen` CLI：生成 Ed25519 密钥对，签名输出兑换码
- [ ] GitHub Secrets 存储 Ed25519 私钥（仅 keygen 使用，不进应用二进制）
- [ ] 激活面板 UI：输入兑换码、显示到期时间和已激活功能
- [ ] 引擎启动时读取激活记录，按 feature bits 控制功能开关
- [ ] 到期前 7 天 UI 提醒（面板 banner + 桌宠状态气泡）
- [ ] 到期后高级功能自动降级（不崩溃，不锁死）
- [ ] 许可证状态面板：剩余天数、激活时间、已授权功能列表

**规则模板**
- [ ] 内置常用规则模板（FPS 快速连发、MOBA 技能连按等），一键导入当前配置

**高级功能**
- [ ] 鼠标连点（按压 / Toggle）
- [ ] 随机抖动（间隔 ± 可配置随机偏差）
- [ ] 宏录制（事件流 + 时间戳，存为 `.qzh`）
- [ ] 宏回放（原速 / 倍速）+ 热键绑定

**发布 v0.4**

---

### v0.5｜落地页 + 运营基础

**目标：** 有对外展示的门面，支撑用户增长。

- [ ] `apps/release-server` Axum 服务，`rust-embed` 内嵌静态资源
- [ ] 落地页 `/`（介绍 + 截图 + 下载按钮 → GitHub Releases）
- [ ] 下载页 `/download`（平台 + 版本信息）
- [ ] 更新日志页 `/changelog`（读取 `changelog.toml`）
- [ ] 健康检查 `/health`
- [ ] 桌宠激活后解锁扩展动画状态

**发布 v0.5**

---

### v1.0｜完整功能

- [ ] 条件配置集（检测活动进程，自动切换配置文件）
- [ ] 回放速度调节 UI（0.5x / 1x / 2x）
- [ ] 桌宠扩展动画包（付费解锁）
- [ ] Mac 兼容（辅助功能权限引导，`rdev`/`enigo` macOS 适配）
- [ ] Azure Trusted Signing 代码签名（GitHub Actions 集成，每次 release 自动签名）；早期版本在安装说明中注明 SmartScreen 绕过方式（"更多信息 → 仍要运行"）

---

### 待定（最低优先级）

- [ ] 桌宠输入响应模式（`gilrs` 手柄 + 键鼠动画反馈，参考 Dongocat）
- [ ] 多平台 release-server 支持（darwin-aarch64、linux-x86_64）

---

## 技术选型

| 用途 | 库 / 工具 |
|------|----------|
| 全局键盘/鼠标监听 | `rdev` |
| 按键/鼠标模拟 | `enigo` |
| 全局热键注册 | `tauri-plugin-global-shortcut` |
| 点击穿透 | `Window::set_ignore_cursor_events()`（Tauri 内置，跨平台） |
| 自动升级 | `tauri-plugin-updater` |
| 配置文件加密 | `aes-gcm` |
| 密钥派生 | `hkdf` + `sha2` |
| 许可证签名校验 | `ed25519-dalek` |
| 兑换码编解码 | `base32` |
| 配置文件变更监听 | `notify` |
| 手柄输入监听 | `gilrs`（阶段三） |
| HTTP 更新服务 | `axum` + `tokio` |
| 应用状态持久化 | `tauri-plugin-store` |
| 开机自启 | `tauri-plugin-autostart` |
| 前端动画 | CSS 关键帧 + SVG（MVP）/ Lottie（后期） |
| 前端 UI | React + TypeScript + Tailwind CSS |
| 多页构建 | Vite multi-page（panel.html + pet.html） |
| Monorepo 管理 | Cargo workspace + pnpm workspaces |

---

## 风险与缓解（已确认）

| # | 风险 | 缓解方案 | 状态 |
|---|------|---------|------|
| ① | rdev + enigo 事件循环 | `AtomicUsize` sim_count 过滤模拟事件 | 确认 |
| ② | 反作弊软件拦截模拟输入 | 不做技术规避，EULA + 文档明确说明 | 确认 |
| ③ | 桌宠被全屏游戏覆盖 | 文档 QA 告知，建议边框全屏 | 确认 |
| ④ | 点击穿透时无法触发右键菜单 | rdev hover 检测，动态调用 `set_ignore_cursor_events()` | 确认 |
| ⑤ | AES 密钥被逆向提取 | 接受"防普通用户"定位，不对抗专业逆向 | 确认 |
| ⑥ | 系统时间回拨绕过许可证 | payload 含 issue_time 做下界校验；`last_verified_at` 列为扩展优化点 | 确认 |
| ⑦ | Named Pipe 连接失败 | 架构改为单一 Tauri 多窗口，风险消除 | 消除 |
| ⑧ | 更新包被中间人替换 | Tauri updater 强制 .sig 签名验证 + HTTPS | 确认 |
| ⑨ | Mac 点击穿透 API 不同 | 改用 Tauri 内置 `set_ignore_cursor_events()`，跨平台，风险消除 | 消除 |
