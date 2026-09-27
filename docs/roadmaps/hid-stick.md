# 外设模式：HID 棒硬件注入 — 改造路线图 / Change Roadmap

Status: active（2026-09-27 立项。棒子固件在 xwink-body 仓先行，本仓 T1 在固件 F5a 验收后开工）
Goal: 助手多一档「外设模式」：按键事件经 USB 串口发给 HID 棒，由棒子作为真实 USB 键鼠敲出；不装驱动、不要管理员、不用重启，目标键等于触发键的切换规则可用。

---

## 1. 背景 / Background

现有三档注入通道都在电脑上生成事件：SendInput 带 `SIM_MARKER`，钩子能认出自己；Interception 与 DD 在驱动层注入，自注入只能靠 `PENDING_INJECTIONS` 的时间窗队列过滤，所以 DD 系列禁止切换规则的目标键等于启动键（`InputMode::requires_distinct_target_for_toggle`），横版布局也因此与它互斥。

HID 棒是 xwink-body 仓做的 ESP32-S3 小板（[xwink-body 具身宿主路线图 D29 到 D32]），USB 复合设备：HID 键盘、HID 鼠标、CDC 串口。它同时是那台机器人的「手」，两个用途共用一个 HID 层。对助手来说它的本质区别是**一台独立的物理设备**：Windows Raw Input 的 `hDevice` 能精确说出每个事件来自哪台键鼠，而 Interception 注入的事件在 Raw Input 里就是用户的真键盘。

| 能力 | 现状 |
| --- | --- |
| 统一分发入口 `win_input::dispatch(KeyId, is_up)` | 已有，按 `(mode, KeyId)` 分发到后端，加一档不动引擎 |
| 后端接口 `send_key / send_mouse / send_wheel` | 已有，三个后端同形 |
| 钩子线程与消息循环 | 已有，`burst-engine` 的 `WH_KEYBOARD_LL` 与 `WH_MOUSE_LL` 共用一条线程 |
| Raw Input 通道 | 缺失 |
| 串口通信 | 缺失，工作区没有串口 crate |
| 棒子指令协议 | 由固件定，以 xwink-body 的 `skills/xwink-body/references/engineering/firmware.md` 的 hand 一节为准 |

不变量：全局总开关关着时不发键；引擎线程异常、切换模式、棒子拔出都不能留下按下未释放的键；`win-*` 不接受 `AppHandle`；配置装载仍只经 `activate_profile_file`。

## 2. 决策 / Decisions

- **D1:** 第四档 `InputMode::HidStick`（序列化名 `hidstick`），后端在 `packages/win-input/src/hidstick.rs` — 理由：`dispatch` 已是统一入口，加一档是最小侵入；引擎与规则层零改动。
- **D2:** 外设模式下触发检测走 Raw Input，按设备句柄过滤，棒子发回的事件不进规则判定；低级钩子仍负责全局热键与需要拦截的场景 — 理由：这是外设模式能做「目标等于触发」而 DD 不能的唯一原因。判定只吃 Raw Input 一条流，不依赖 `WM_INPUT` 与钩子回调谁先到，两者都由 RIT 派发、顺序没有文档保证。
- **D3:** 通道用 CDC 串口行协议，每个注入事件一条指令，连发调度仍在电脑 — 理由：引擎不动；usbser 写入约 1 ms 加 HID 中断端点 1 ms 轮询，总时延两三毫秒，远小于 10 ms 的有效间隔下限。助手按 VID/PID 找口，不让用户选。
- **D4:** 断开即松键两端都做：棒子 DTR 掉或心跳超时松开全部；助手切出外设模式或棒子拔出时清状态、回退 SendInput 并 warn 一次 — 理由：卡键是这类外设最常见的事故。
- **D5:** 不伪装 VID/PID，不承诺反检测；README 末尾的风险提示对外设模式同样成立 — 理由：冒用别家号是另一件事；能被查的是节奏不是设备，硬件 HID 与外设厂商的宏键盘同类。
- **D6:** 外设模式不与横版布局互斥，`requires_distinct_target_for_toggle` 对它返回 `false` — 理由：互斥的根源是 DD 下重合态切换连发是空集，D2 解决了这个前提。
- **待拍板 P1（不阻塞任何卡）：** 外设模式算不算亲友专属功能（[license](./license.md) 的 feature bit）。推荐默认：不算，棒子本身就是门槛，助手侧保持免费。

## 3. 问题清单 / Findings

| # | 问题 | 严重度 | 位置 | → 任务卡 |
| --- | --- | --- | --- | --- |
| I1 | `PENDING_INJECTIONS` 时间窗对目标等于触发的切换规则不可靠，会自停或停不掉 | P1 | `packages/win-input/src/lib.rs:239` | T2 |
| I2 | 棒子拔出或串口写失败后若继续下发，规则静默失效且可能留下按下未释放 | P1 | `packages/win-input/src/hidstick.rs`（新） | T1 |
| I3 | 设置弹窗与 `requires_admin` 都假设只有三档 | P2 | `apps/main/src/windows/panel/dialogs/SettingsDialog.tsx`、`packages/win-input/src/lib.rs:243` | T3 |
| I4 | 说明书只讲三档通道 | P2 | `skills/flair-bloom/references/manual/game-mode.md` | T3 |

## 4. 任务卡 / Task cards

### T1 — 外设模式后端 · 进度: 未开始
- **Fixes:** I2
- **Change:** `win-input` 加 `hidstick.rs`：按 VID/PID 枚举串口并打开；`send_key / send_mouse / send_wheel` 翻译成棒子的行协议，VK 到 HID usage 一张表，鼠标五键与滚轮原生；后台心跳；写失败即标记离线、清挂起注入、回退 SendInput 并 warn 一次。`InputMode` 加 `HidStick`（`from_str` / `as_str` / `requires_admin` 返回 `false`），`init_backend` 加分支。工作区引入一个串口 crate。
- **Acceptance:** 外设模式下长按连发与切换连发能跑；拔掉棒子后一秒内引擎无卡键、日志一条 warn、模式回退。
- **Test:** 单测覆盖 usage 表（字母、数字、功能键、修饰键、小键盘）、协议编码、离线回退路径；真机：记事本里长按 Q 连发。
- **Rollback:** 设置切回任一现有模式即可，不动配置文件 schema。
- **Depends on:** xwink-body F5a（固件与协议定稿）。

### T2 — Raw Input 检测通道 · 进度: 未开始
- **Fixes:** I1
- **Change:** `burst-engine` 在钩子那条消息循环线程建 message-only 窗口，注册键盘与鼠标的 `RIDEV_INPUTSINK`，事件带设备路径；外设模式下 `on_key_press / release` 的输入源改为 Raw Input 并过滤棒子设备，钩子只保留热键与需要拦截的规则；`requires_distinct_target_for_toggle` 对 `HidStick` 返回 `false`，布局互斥判断不含它。非 Windows 保持空实现。
- **Acceptance:** 目标等于触发的切换规则在外设模式下起停正常；互斥组插队栈不被棒子的回灌误触发。
- **Test:** 单测：设备过滤与事件到规则的映射用假事件流；真机按 [testing](../../skills/flair-bloom/references/engineering/testing.md) 的冒烟清单过三类规则加互斥组。
- **Rollback:** 只在 `HidStick` 模式启用 Raw Input 路径，其余模式行为不变。
- **Depends on:** T1。
- **逼出 / Why:** 引擎第一次拥有「事件来自哪台设备」这个信息，DD 的时间窗约束以后也可能靠它放宽。

### T3 — 设置、状态与说明书 · 进度: 未开始
- **Fixes:** I3、I4
- **Change:** 设置弹窗加「外设模式」档位，状态区显示棒子在线与否；`manual/` 加 `hid-stick.md` 并按 `pnpm skills:check` 补对应教程条目；README 通道说明与风险提示各补一句。
- **Acceptance:** 用户不看源码能按说明书把棒子用起来；`pnpm skills:check` 通过。
- **Test:** `pnpm skills:check`、`pnpm test:ui`。
- **Depends on:** T1。

### T4 — 真机冒烟 · 进度: 未开始
- **Change:** 在测试配置里按说明书走一遍：三类规则、互斥组、插队、拔棒子、切模式。
- **Acceptance:** 冒烟清单全绿。
- **Depends on:** T2、T3。

## 5. 依赖与顺序 / Dependencies & order

```
xwink-body F5a ─> T1 ─> T2 ─> T4
                  └──> T3 ─┘
```

- **执行顺序 / Order:** 固件 F5a → T1 → T3 → T2 → T4。
- **最小可交付里程碑 / Minimum valuable milestone:** T1 加 T3。外设模式能连发、不装驱动；切换规则暂沿用 DD 的约束，T2 才解锁目标等于触发。

## 6. 工期 / Effort

口径：单人加 AI 协作，单位「会话」，与 [onboarding-tour](./archive/onboarding-tour.md) 同口径。校准来源：那份路线图 6.5 会话按预估完成。

| 任务卡 | 预估 | 实际 | 偏差说明 |
| --- | --- | --- | --- |
| T1 | 1 会话 | | |
| T2 | 1.5 会话 | | |
| T3 | 0.5 会话 | | |
| T4 | 0.5 会话 | | |
| **合计** | **3.5 会话** | | |

拉长日历但不占净工时：固件 F5a 先行；送人的棒子换小板（S3 SuperMini 或 S3 Zero）要再采购。

## 7. 风险与回滚 / Risks & rollback

- **R1 Raw Input 与钩子看到同一事件的先后与重复**：判定只取 Raw Input 一条流，钩子路径不再对棒子回灌做判断，就不存在对账。
- **R2 CDC 在部分机箱前面板口握手失败**（实测过描述符请求失败）：说明书写明插主板后置口；助手枚举不到棒子时提示这一点。
- **R3 反作弊按行为检测**：与现有三档同一风险，D5 不做承诺。

## 8. 范围外 / Out of scope · follow-ups

- 键盘经棒子中转的中间人形态（棒子加 MAX3421E 当 USB 主机，规则全跑在片上）。
- 绝对坐标鼠标（HID 数位板描述符）。
- 助手经 CDC 给棒子升级固件。
