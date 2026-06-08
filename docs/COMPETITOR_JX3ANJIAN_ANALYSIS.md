# 竞品调研：jx3anjian

调研对象：<https://github.com/mh58373397-byte/jx3anjian>

调研提交：`c01506573a46923e757ca8842c7ad90af6477384`

调研日期：2026-05-31

本地源码：`target/competitor-jx3anjian`

## 1. 总览

`jx3anjian` 是一个 Win32 C 单体应用，主逻辑集中在 `main.c`，宏功能集中在 `macro.c`。README 主打 Interception 驱动级连发，但代码实际包含两套输入通道：

| 通道 | 代码枚举 | 作用 |
| --- | --- | --- |
| DD64 | `DRV_DD = 0` | 使用内嵌 `dd64.dll`，通过 `DD_btn` / `DD_key` / `DD_whl` 注入 |
| Interception | `DRV_INTERCEPTION = 1` | 使用内嵌 `interception.dll`，可拦截、改写、转发物理输入流 |

产品形态偏“游戏脚本工具”：功能覆盖广，支持宏、换键、独占、WASD 稳定、极速模式和悬浮窗；工程形态偏“单体实用实现”：大量全局状态、手写 JSON、hook 内逻辑较重，维护和安全边界弱于我们。

## 2. 驱动调用分析

### 2.1 资源释放与 DLL 装载

竞品把驱动相关 DLL/安装器编进 exe 资源，运行时释放到 `%TEMP%`：

- `interception.dll`：释放后 `LoadLibraryW`，解析 `interception_create_context`、`interception_send`、`interception_receive` 等函数。
- `dd64.dll`：释放为 `dd64_autokey.dll`，解析 `DD_btn`、`DD_whl`、`DD_key`、`DD_mov`、`DD_str`、`DD_todc`、`DD_movR`。
- Interception 安装器：释放 `install-interception.exe`，通过 `/install` 或 `/uninstall` 提权执行。

DD 通道用 `DD_btn(0) == 1` 判断驱动就绪。这个协议点和我们当前 DD-HID 代码一致。

风险点：

- 临时目录释放文件没有哈希校验。
- 资源覆盖写入固定文件名，理论上受临时目录污染、杀软拦截、残留文件影响。
- 安装状态判断偏“能不能打开 context / 能不能 DD_btn(0)”，没有我们这种资源完整性、服务键、sys 文件、PnP 残留的综合诊断。

### 2.2 DD64 注入路径

竞品 DD 键盘路径：

1. `DD_todc(vk)` 把 VK 转 DD 键码。
2. `DD_key(ddcode, 1)` 表示按下。
3. `DD_key(ddcode, 2)` 表示弹起。

竞品 DD 鼠标路径：

| 目标 | `DD_btn` flag |
| --- | --- |
| 左键按下/弹起 | `1` / `2` |
| 右键按下/弹起 | `4` / `8` |
| 中键按下/弹起 | `16` / `32` |
| 侧键 1 按下/弹起 | `64` / `128` |
| 侧键 2 按下/弹起 | `256` / `512` |

`DDREADME.md` 明确写了 4 键和 5 键，也就是 Windows X1/X2 侧键。竞品宏回放里也会把 `INTERCEPTION_MOUSE_BUTTON_4_DOWN/UP` 和 `BUTTON_5_DOWN/UP` 传给统一的 `drv_send_mouse_btn`，在 DD 模式下等价于上述 flag。

这说明至少竞品使用的 DD64 SDK 支持鼠标侧键。注意：这条结论不能直接外推到我们打包的 2026 HVCI `ddhid.63340.dll`。

### 2.2.1 2026 HVCI 63340 原包复核

用户提供的原始包 `C:\Users\ADMIN\Downloads\2026.DD.EV.HVCI.63xxx` 已复核：

- 原包 `2.hid\ddhid.63340.dll` 与我们打包的 `apps/main/src-tauri/resources/ddhid.63340.dll` SHA256 一致：`01E8DB6893CF79E9E7AA3AFBEE76BEA6C4220C4D1A2C63BC2E5B7C109FDB831E`。
- 原包 `2.hid\drv\ddhid63340.sys` 与我们打包的 `apps/main/src-tauri/resources/ddhid-driver/ddhid63340.sys` SHA256 一致：`FBE510402B3822C63E94752051B7D5895B67875F22EC48593DE19764A649F8B1`。
- 原包多语言示例只写到 `1/2/4/8/16/32`，也就是左/右/中键；没有 `64/128/256/512` 的侧键示例。
- `ddhid.63340.dll` 只导出 `DD_btn`、`DD_key`、`DD_mov`、`DD_movR`、`DD_str`、`DD_todc`、`DD_whl` 7 个老接口，没有侧键专用导出。
- `ddhid63340.sys` 的 HID mouse report descriptor 声明 `Usage Min 1` 到 `Usage Max 5`，`Report Count 5`，驱动层是 5 键鼠标。
- 静态拆解 `DD_btn` 可见：63340 DLL 内部 switch 只处理 `1/2/4/8/16/32`，只更新按钮状态位 `0x01/0x02/0x04`；`64/128/256/512` 会落入默认发送路径，不会更新 X1/X2 的 `0x08/0x10` 状态位。因此“只传竞品 DD64 flag”在 63340 上不会触发侧键。

结论：竞品 DD64 的侧键协议与 2026 HVCI 63340 DLL 不完全兼容。我们后续实现不能只照搬 `DD_btn(64/128/256/512)`，需要在已确认的 63340 版本上补写 DLL 内部鼠标按钮状态位，再复用 DLL 自己的 HID report 写入路径。

### 2.3 Interception 注入与拦截路径

竞品 Interception 路径有两个层次：

1. 注入：构造 `InterceptionKeyStroke` / `InterceptionMouseStroke` 后 `interception_send`。
2. 拦截：启动 `intercept_proc`，调用 `interception_set_filter` 捕获键盘全键、鼠标按钮和滚轮，再决定是否 `forward_original`。

关键能力来自第二点。它不是只“模拟输入”，而是能在物理输入流上做：

- 吞掉原始事件。
- 替换扫描码和 E0 flag，实现驱动层换键。
- 捕获宏热键，绕过部分游戏焦点下 `RegisterHotKey` 失效的问题。
- 在同一个输入流里维护 Hold/Toggle/Hybrid 状态。

弱点是 Interception 注入 stroke 的 `information = 0`，没有类似我们 `SIM_MARKER` 的自注入标记。因此它更依赖拦截路径的上下文、DD skip counter、前台窗口判断和状态机来避免自触发。

## 3. 连发引擎实现

### 3.1 活动槽模型

竞品维护：

- `MAX_ACTIVE = 16`
- `g_aslots[MAX_ACTIVE]`
- `g_held[]`
- `g_toggled[]`
- `g_active_count`
- `CRITICAL_SECTION g_cs_active`

输入 hook 或 Interception 拦截线程负责更新 held/toggled 状态和活动槽；`repeat_proc` 是唯一连发线程，循环复制活动槽快照，然后依次给每个活动键发送 down/up。

### 3.2 调度与精度

`repeat_proc` 启动后：

- `timeBeginPeriod(1)` 提升系统 timer 粒度。
- `SetThreadPriority(..., THREAD_PRIORITY_TIME_CRITICAL)` 设置时间关键优先级。
- 延迟函数 `interruptible_sleep_us` 先 `Sleep` 分块等待，再用 QPC + `YieldProcessor` 忙等。
- 普通模式下有效延迟至少 1000us。
- 极速模式下最低 1us。

优点：

- 极致简单，延迟下限很低。
- 在轻负载、短时间内可以打出非常高频率。

缺点：

- 忙等和 time-critical 线程可能抢占系统资源。
- 高频下 CPU 占用和系统抖动不可控。
- 只有 16 个活动槽，上限低于我们 64 条规则。
- hook、拦截线程、repeat 线程共享大量全局状态，竞态面较大。

### 3.3 WASD 稳定模式

竞品单独跟踪 W/A/S/D 按下状态：

- 任一 WASD 按下时，连发间隔下限提升到 `g_wasd_stable_delay_ms`，默认 30ms。
- 宏每轮循环末尾额外等待同样的稳定延迟。
- WASD 全部弹起后恢复原速率。

这是一个很实用的游戏场景优化。它牺牲连发速度，换取移动输入更稳定。

## 4. 宏引擎实现

竞品宏功能在 `macro.c`：

- `MAX_MACRO_EVENTS = 50000`
- `MAX_MACRO_CMDS = 50000`
- `MAX_MACRO_SLOTS = 20`
- README 写最多 5 组宏，但代码实际支持 20 槽。

录制来源：

- Interception 拦截路径记录键盘扫描码、鼠标按钮、滚轮、时间戳。
- DD/hook 路径也有宏录制入口。
- 鼠标移动可按 100ms timer 采样。

回放方式：

- 解析中文脚本文本为 `MacroStep`。
- 单独创建 playback 线程。
- 调用统一驱动封装：`drv_send_key_scan`、`drv_send_mouse_btn`、`drv_send_mouse_move_abs`、`drv_send_mouse_wheel`。
- 播放结束、停止、关闭窗口时调用 `reset_playback_pressed_inputs()` 释放宏按住的键鼠。

宏能力强于我们当前主线。我们的路线图中有宏序列和宏播放规划，但当前产品主能力仍是规则连发。

## 5. 边界行为与异常兜底

竞品做了不少实战兜底：

- `RegisterHotKey` 失败后启用 hook fallback。
- DD 注入前递增 `g_dd_skip_dn/up`，低级 hook 收到后消费计数，避免把自身注入当物理输入。
- Interception 模式下宏热键走拦截线程，避免游戏焦点导致系统热键失效。
- 全局停止、切模式、退出时释放 Toggle。
- 宏停止时释放 playback 中按住的键鼠。
- 当前窗口限定模式可在离开目标窗口后暂停。
- 独占排除键可以静默暂停/恢复其它 Toggle。
- 配置加载后清理与热键冲突的独占配置。
- DD 同键 Toggle 依赖 `g_dd_skip_dn/up` 过滤自身注入，并用 `g_held[]` 避免物理长按 repeat 重复切换；它能支撑 `trigger == target == stop` 的产品形态，但不是驱动层可证明的绝对标记。

主要缺口：

- 没有强资源完整性校验。
- 没有结构化驱动诊断/修复报告。
- Interception 注入没有 marker。
- DD skip counter 属于计数兜底，复杂重入或丢事件时可能误消费。
- 手写 JSON 解析对格式漂移比较脆弱。
- 单体全局状态多，调试复杂。

## 6. 功能点列表

竞品已实现或代码中存在的功能：

- Hold 按住连发。
- Toggle 指定连发。
- Hybrid 混合模式。
- 键盘 + 鼠标五键连发。
- 鼠标滚轮宏录制和回放。
- 微秒级延迟和极速模式。
- 可视化键盘布局。
- 拖拽键位对调。
- 换键可独立于连发生效。
- 独占模式。
- 排除键。
- 当前窗口限定。
- WASD 稳定输入模式。
- 技能闪烁顺序，也就是 up/down 顺序切换。
- 宏录制、脚本编辑、回放、按压模式、无限循环。
- 多宏槽。
- 全局启停热键，默认 F1。
- 游戏悬浮窗热键，默认 F9。
- 半透明无边框置顶悬浮窗。
- MP3 音效和音量。
- 暂停模式。
- 多 JSON 配置文件。
- 驱动安装/卸载入口。

## 7. 与我们 APP 对比

| 维度 | 竞品 | 我们 APP |
| --- | --- | --- |
| 架构 | Win32 C 单体应用 | Tauri + Rust workspace，模块拆分清晰 |
| 输入模式 | Interception + DD64 | SendInput + Interception + DD-HID |
| 物理输入处理 | Interception 可拦截、吞掉、改写物理流 | 低级 hook 观察物理输入，默认继续转发 |
| 自注入过滤 | DD skip counter；Interception 无 marker | SendInput/Interception 使用 `SIM_MARKER`，DD-HID 使用 pending/relay 队列 |
| 调度模型 | time-critical 线程循环扫描活动槽 | 单 scheduler + mpsc command + 高精度 waitable timer |
| 延迟下限 | 极速模式 1us | 产品约束最低 1ms，默认 10ms |
| 活动/规则上限 | 活动槽 16 | 规则上限 64 |
| 宏 | 已有完整录制/回放 | 规划中，当前不是主线能力 |
| 换键 | Interception 拦截层实现全局换键 | 当前无真实驱动层 remap |
| 独占/WASD | 已有 | 暂无完整等价功能 |
| 鼠标侧键 DD | DD64 文档和实现支持 X1/X2 | 63340 原包 `DD_btn` 不直接支持侧键；本次改为补写内部 5 键 report 状态位后复用 DLL 写入路径 |
| 驱动诊断 | 基本安装/卸载 | 资源哈希、状态三态、残留修复、报告导出更强 |
| 安全性 | 临时释放资源，无哈希校验 | 包内资源 SHA256 固定清单，安装前校验 |
| 可维护性 | 功能密集但全局状态多 | Rust 类型约束和模块边界更好 |

## 8. 可借鉴方向

优先建议借鉴：

1. WASD 稳定输入模式。实现成本低，游戏收益明确，可作为高级选项。
2. 独占模式。对 Toggle 之间的互斥、打断、恢复很有价值，但需要设计好状态机，避免规则之间相互污染。
3. 宏录制与回放。竞品已经证明用户价值高，但我们应使用结构化 `MacroStep` schema，不照搬中文脚本文本解析。
4. Interception capture layer。若要做换键/吞键，需要新增独立拦截层，不宜塞进现有注入后端。
5. DD-HID 同键 Toggle。竞品证明“同键启动 / 连发 / 停止”是用户可理解的主流形态；我们可以用 pending injection 队列、relay 队列、停止键优先级和全局开关兜底做得比竞品更稳。
6. DD 侧键支持。竞品 DD64 flag 不能直接用于 63340；当前采用 63340 专用状态位补写方案，已由用户实机确认游戏内生效。

不建议照搬：

- 1us 忙等调度和 time-critical 常驻线程。
- 临时目录释放 DLL 且无完整性校验。
- 手写 JSON 配置解析。
- hook 内混入大量业务分支。
- 无 marker 的 Interception 自注入路径。

## 9. 本次对我们 APP 的调整建议

短期改动：

- DD-HID `send_mouse` 针对 2026 HVCI `ddhid.63340.dll` 增加 X1/X2 状态位补写：X1 使用 HID button bit `0x08`，X2 使用 `0x10`，再调用 `DD_btn(64/128/256/512)` 进入 DLL 默认发送路径。
- 移除 profile 和 Tauri 命令层对 DD-HID 侧键目标的禁止。
- 移除 DD-HID 下 Toggle `target_key == trigger_key / stop_key` 的限制，允许 `trigger == target == stop`。
- 引擎层将活动 Toggle 的停止键优先级提升到最高：某次 keydown 只要能停止正在运行的 Toggle，就不再用同一个 key 启动其它规则。
- README 和 ROADMAP 更新为“侧键走 63340 的 5 键 HID report 写入路径；DD-HID 同键 Toggle 由 pending/relay 过滤、停止优先级和全局开关兜底”。

同键 Toggle 讨论结论：

- 竞品无法 100% 证明“永不误停 / 永不停不掉”，因为 DD 注入没有可靠 `dwExtraInfo` marker，只能靠 skip counter 猜测近期注入。
- 我们可以在实践稳定性上高于竞品：DD-HID 注入前记录 pending，WebView relay 侧单独记录 relay pending，停止路径有 lifecycle/generation 闸门、simulated ledger 与 500ms StopAll fallback。
- 产品侧不再对 `trigger == target == stop` 做风险提醒或配置阻断；全局开关保留为最终停止兜底。
- 后续若要进一步增强，可研究 Raw Input 区分 DD-HID 虚拟设备与物理设备，但不作为当前放开的前置条件。

实测结论：

- DD-HID 模式下 X1 目标键已由用户确认在游戏内生效。
- 后续替换 DD-HID 驱动/DLL 版本时，需要重新确认 63340 专用侧键状态位补写是否仍适用。
