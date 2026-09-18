# 桌宠模式 — 设计稿

Status: draft
Goal: 桌面上一只随连发状态动的小东西，提升差异化与趣味性；开工时转 active 并补任务卡与工期。

---

## 1. 背景

当前只有面板与浮窗两个窗口，`pet.html` 未创建。桌宠是第三个 WebView：透明无边框、始终置顶、默认点击穿透，通过动画反映连发状态。应用模式与窗口通信见 skill 的 [architecture](../../skills/flair-bloom/references/engineering/architecture.md)。

## 2. 决策

- **D1：点击穿透用 Tauri 内置 `Window::set_ignore_cursor_events(bool)`** — 理由：跨平台（Windows / macOS 均支持），不为桌宠再引入与连发引擎无关的全局输入依赖。光标进入桌宠区域时关闭穿透（支持拖拽和右键），离开恢复；鼠标位置监听优先用 Tauri / 系统 API。
- **D2：窗口配置** — `tauri.conf.json` 增 `pet` 窗口：`transparent` / `decorations: false` / `alwaysOnTop` / `skipTaskbar` / `visible: false` / 160×160；Vite 增 `pet.html` 入口。
- **D3：动画资源 MVP 用 CSS 动画 + SVG**，后期视美术资源升级为 Sprite Sheet 或 Lottie。
- **D4：状态来源复用 `global-enabled-changed` / `app-status-changed` 事件**，为激活规则补统一事件（当前面板靠轮询 `get_active_rules`）。

## 3. 设计

### 动画状态机

| 状态  | 动画                   | 触发                      |
| ----- | ---------------------- | ------------------------- |
| Idle  | 缓慢呼吸，偶尔眨眼     | 默认                      |
| Burst | 快速抖动或奔跑循环     | 连发引擎激活              |
| Hover | 抬头看向光标，尾巴摇动 | 鼠标进入窗口区域          |
| Alert | 耳朵竖起，眼睛放大     | 切换配置文件              |
| Sleep | 闭眼 ZZZ               | 空闲超过 N 分钟（可配置） |

### 交互

- 拖拽：关闭穿透后按下拖动，位置存 `settings.json`，重启恢复。
- 右键菜单：开关连发 / 切换配置 / 打开面板 / 退出。
- 左键单击：状态气泡（当前规则、许可证到期），3 秒淡出。
- 托盘菜单增「打开 / 关闭桌宠」。

### 前端结构（拟）

`apps/main/src/windows/pet/`：`PetApp.tsx`、`components/PetCanvas`、`components/StatusBubble`、`hooks/useEngineStatus.ts`、`hooks/usePetAnim.ts`。

### 扩展：输入响应动画（最低优先级）

参考 Bongo Cat，监听全局键盘、鼠标、手柄输入做动画反馈，与连发引擎解耦：Typing（连续输入 >2 次/秒）、KeyPress、MouseMove（眼睛跟随）、Click（眨眼）、GamepadButton、GamepadStick（身体倾斜）。手柄用 `gilrs`，需配套美术资源后再评估。

## 4. 已知风险

- 全屏游戏会覆盖桌宠：实现时在文档 QA 告知，建议无边框全屏。
- 穿透态无法触发右键菜单：靠 D1 的动态切换解决。

## 5. 待办（开工时展开成任务卡）

- [ ] pet 窗口配置与 `pet.html` 入口
- [ ] 点击穿透与光标区域检测
- [ ] 拖拽 + 位置持久化
- [ ] SVG 角色 + CSS 动画（Idle / Burst / Hover），再补 Alert / Sleep
- [ ] `useEngineStatus` / `usePetAnim`
- [ ] 右键菜单、左键状态气泡、托盘开关
- [ ] 亲友专属扩展动画包（依赖许可证）
