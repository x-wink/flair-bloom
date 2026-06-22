# 自动化测试策略

按键助手的测试难点在于：连发依赖低级 hook（`WH_KEYBOARD_LL`/`WH_MOUSE_LL`）与驱动注入（SendInput / DD / Interception），这些跑在 OS 输入栈上、需要交互式桌面会话，传统 headless CI 跑不了。业界同类工具（[kanata](https://github.com/jtroo/kanata) 的 `simulated_input`、[PowerToys Keyboard Manager](https://github.com/microsoft/PowerToys) 的 `Input` 抽象）的共识是**分层**：能在不碰 OS 的前提下确定性验证的逻辑，全部下沉到模拟测试；真正碰 OS/驱动的那一层才手动或半自动验。

## 三层

| 层 | 测什么 | 怎么测 | 跑在哪 | 状态 |
| --- | --- | --- | --- | --- |
| **L1 引擎逻辑（确定性）** | 连发时序、Hold/Toggle/分组/停止/热键/共享目标/throttle | 虚拟时钟 + 录制 dispatcher，纯 Rust 单测 | CI（`ci.yml`，windows-latest） | ✅ 已落地 |
| **L1.5 引擎管线** | 按键 → 引擎 → 下发给调度器的命令 + generation | 命令录制替身 | CI（同上） | ✅ 已落地 |
| **L2 真 OS 冒烟** | SendInput → LL hook → SIM_MARKER 往返 | 自装 hook 捕获并吞掉自注入 | 本机 / 自建 Windows runner（`#[ignore]`） | ✅ SendInput 已落地；DD/Interception 待真机 |
| **L3 驱动/反作弊** | DD/Interception 真驱动是否打进游戏 | 手动 | 真机 | 不可自动化 |

**核心认知**：当前发版前手动验的边界，80–90% 是 L1/L1.5 引擎逻辑——这些已自动化、已进 CI 门禁。真正不可自动化的只剩 L3「注入有没有真打进游戏」，竞品也是手动验。

## CI 门禁

`.github/workflows/ci.yml`（push:main / PR 触发，windows-latest）：`cargo fmt --check` + `cargo clippy -D warnings` + `cargo test`（排除 Tauri app `flair-bloom`，其构建依赖前端 dist；逻辑都在各 crate）。L1/L1.5/引擎/边界用例每次推送自动跑。L2 冒烟为 `#[ignore]` 不在 CI 跑（需交互式会话）。共享 crate 覆盖率另由 `coverage.yml` 把守 ≥85%。

## L1 已落地：调度器模拟 harness

代码：`packages/burst-engine/src/scheduler/sim_tests.rs`。

原理：连发时序逻辑全在 `SchedulerWorker::process_due(now)` 里，且它**把当前时刻作为入参**。喂合成的 `now`（虚拟时钟，逐毫秒推进）+ 一个只记录事件的 `RecordingDispatcher`（替代真实注入），即可对注入事件序列做 golden 断言——零 OS、完全确定性。

脚本 DSL（空白分隔）：

| token | 含义 |
| --- | --- |
| `start:<id>` | 启动规则 |
| `stop:<id>` | 停止规则 |
| `stopall` | 停止全部 |
| `t:<ms>` | 推进虚拟时间 N 毫秒 |

输出：`<毫秒>:dn:<键>` / `<毫秒>:up:<键>`，按 (时刻, 抬起, 键) 稳定排序后空格连接。键渲染：键盘 `K<十六进制VK>`，鼠标 `M<按钮名>`。

**加一个场景测试 = 一行脚本 + 一个期望串**：

```rust
#[test]
fn hold_mode_separates_down_and_up_by_hold_duration() {
    let out = simulate(vec![hold("r", E, 10)], "start:r t:25");
    assert_eq!(out, "1:dn:K45 4:up:K45 11:dn:K45 14:up:K45 21:dn:K45 24:up:K45");
}
```

### L1 当前覆盖

- 调度器层（`scheduler/sim_tests.rs`）：点按档(1ms)同拍 down+up、Hold 档 down/up 按时长分离、节拍稳定、interval 边界(2ms 仍有 1ms 按下)、停止释放按住的目标键、间隔期停止不补多余 up、共享目标只发一次 down、共享目标 stopall 只释放一次、多目标 stopall 全释放、停止其一不影响其余、停止后干净重启、鼠标目标键。
- 引擎状态机层（`burst-engine/src/lib.rs` `#[cfg(test)] mod tests`）：Hold 首按启动/松开停止、Toggle 切换、Toggle 独立停止键、分组互斥顶替 + 顶替后复位重启、热键长按去重、鼠标键去重、专用停止键先于去重 100% 触发、停止复位物理账本、全局禁用拦截启动。
- 引擎管线层（`burst-engine/src/pipeline_tests.rs`）：注入命令录制替身，断言「按键 → 引擎 → 实际下发给调度器的命令 + generation」——Hold 启停、Toggle 同键开关、分组顶替先 stop 旧再 start 新、关全局开关 / 专用停止键下发 stop_all 且 generation 递增。抓 `active_ids` 测不到的引擎↔调度契约 bug。
- 纯逻辑层（共享 crate，覆盖率门槛 ≥85%）：`KeyId`、profile 校验、schema 迁移、AES/Ed25519、注入回灌过滤队列、throttle 升降数学。

运行：`cargo test -p burst-engine`（含 sim_tests）。

## L2 已落地：真 OS 往返冒烟

代码：`packages/burst-engine/src/smoke_tests.rs`（`#[cfg(all(test, windows))]` + `#[ignore]`）。

自装一个 `WH_KEYBOARD_LL` hook，对带 `SIM_MARKER` 的自注入事件**捕获后直接吞掉**（返回 1，不下传）——既验证 `win_input` SendInput 注入 → LL hook → 还原出正确 vk/抬起/`SIM_MARKER` 整条真实链路，又**零副作用**（注入键不泄漏到前台窗口，物理键照常放行）。需交互式桌面会话，故 `#[ignore]`：

```sh
cargo test -p burst-engine -- --ignored
```

**仍待真机/真驱动**：DD-HID / DDSimple / Interception 后端的往返需装对应驱动（且 DD 系列需管理员），同样的 hook + 吞自注入框架可扩展过去，但 GitHub 托管 runner 装不了驱动，需自建带驱动的 Windows runner，或在发版前本机手动跑。

> L1.5（引擎管线确定性）已落地：`BurstEngine` 的调度器抽象成 `scheduler::Scheduler` trait，测试经 `BurstEngine::new_with_scheduler` 注入命令录制替身，断言引擎下发的命令序列。见 `pipeline_tests.rs`。

## 已知不可自动化的边界

- **DD 同键 Hold（按住 Q 连发 Q）偶发自停**：DD 驱动清零 `ExtraInformation`，`SIM_MARKER` 无法幸存，hook 只能靠 `PENDING_INJECTIONS` 时间窗口尽力过滤，对同键无法可靠区分自注入与物理松手。这是 DD 驱动固有缺陷（竞品同样无解），L1 测不出，L2 sink 能观测到泄漏但做不成确定性断言。彻底消除需 Raw Input 按设备来源区分虚拟/物理设备。
