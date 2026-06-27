# Windows 实测待办

非 Windows 环境无法复现 / 验证的问题登记于此，待在真实 Windows 环境实测确认根因后再修复。
确认并修复后，把对应条目移除，并按需补进 `CHANGELOG.md` 的「问题修复」。

---

## 1. 窗口前置时按全局热键切「全局启用」总是停在「开启」

- **状态**：根因待 Windows 实测定位（已排除一个假设，见下）
- **报告来源**：用户反馈（2026-06-27）
- **平台代码**：`#[cfg(windows)]`，本机（macOS）无法复现

### 症状

应用窗口处于前置 / 聚焦状态时，按全局开关热键（`global_toggle`）切换「全局启用」，每次都被设为「开启」、无法切到「关闭」（总是提示 / 播报启用）。窗口**未**前置时切换正常。

### 已确认的前提：聚焦时只有前端 relay 单路径（不是双触发）

由本仓库 git 历史权威确认（非推测）：

- `b91def4`「WebView2 聚焦后 **Chromium hook 截断 WH_KEYBOARD_LL**，调度器通过驱动注入的模拟按键同样产生 DOM 事件并被 relay_key_event 当作物理按键中继」
- `e9f5ce1`「面板聚焦时全局热键（global_toggle / global_stop / panel_toggle）由前端 keydown 补充处理，**绕过 Chromium WH_KEYBOARD_LL 干扰**」

即：面板聚焦时 Chromium 拦掉低级键盘 hook，Rust 后端 `keyboard_hook_proc` 收不到，所以才有前端
`PanelApp.tsx` 的 keydown → `relay_key_event` 中继。**聚焦时是 relay 单路径**，后端 hook 不参与。

> 因此「hook 与 relay 双重处理同一次按键」的旧假设作废——聚焦时根本没有 hook 那一路。

### 矛盾点：单路径 toggle 逻辑读代码是正确的

把聚焦单路径从头到尾推演（按代码现状）：

1. DOM keydown → `relay_key_event`（`commands/engine.rs:100`）→ `try_consume_relay_injection` 过滤注入回灌（物理键不受影响）→ `on_key_press_event`（`lib.rs:216`）
2. `physical_pressed.insert(key)` → true（首次按下，未被去重）
3. `handle_hotkey_press`（`lib.rs:338-361`）是干净取反：`start==key && !enabled → 开`；`stop==key && enabled → 关`（`global_stop` 为空时 `stop` 回退 `start`）
4. keyup → relay up → `on_key_release_event` → `physical_pressed.remove(key)`

**单路径下「开→关→开」应正常交替**。所以「总是停在开启」无法只用单路径代码逻辑解释，必然还有一个运行时才暴露的重复 / 干扰来源，需在 Windows 上抓现场。

### Windows 上要抓什么（按怀疑度排序）

1. **relay 是否对一次物理按下发了不止一次 down**：在 `downHandler`（`PanelApp.tsx:671`）和 `relay_key_event`（`commands/engine.rs:100`）各打一行日志（含 key、isUp、`e.repeat`）。聚焦时按一次热键，看 `relay_key_event(down)` 是否被调用了两次。
   - React `<React.StrictMode>`（`main.tsx:50`）在 **dev** 下会重复挂载 effect，可能叠加监听器——务必用 **release 构建**复现，排除这一干扰。
2. **是否注入回灌被当成物理按键**：开「全局启用」后规则注入的按键在聚焦时会产生 DOM 事件，确认 `RELAY_INJECTIONS` / `try_consume_relay_injection`（`win-input` + `commands/engine.rs:101`）是否把它们全过滤干净；漏过的话会被当物理键再次进 `on_key_press_event`。
3. **抓 `global_enabled` 的实际跳变序列**：在 `set_global_enabled_and_notify`（`lib.rs:331`）打日志，记录每次按键后的 true/false 序列，验证到底是「设了关又被设回开」还是「压根没进关分支」。
4. 确认 `global_stop` 配置：是否设了独立停止键（影响 `dedicated_stop` 快路径 `lib.rs:223-233` 与 `stop` 回退）。

定位后应补一个聚焦 relay 单路径的回归测试（现有 `repeated_keydown_does_not_retrigger_global_toggle_before_release` 只覆盖单次 down 去重，未覆盖聚焦中继场景）。
