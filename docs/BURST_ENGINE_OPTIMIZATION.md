# 连发引擎安全与性能优化任务

## 目标

在不破坏按压连发、Toggle 连发、鼠标连点、滚轮连发和三档输入后端的前提下，降低 hook 热路径成本，减少极端配置下的线程与调度开销，并把异常兜底行为统一到可验证的安全模型中。10ms 连发是主要使用场景，scheduler 必须优先优化低间隔下的节拍稳定性。

## 不可退让原则

- 物理输入永远优先：hook 只观察输入并维护状态，不吞键、不阻断鼠标事件。
- 同键 Hold 是核心用法：`Hold trigger_key == target_key` 表示把用户按住转换为周期性 `down/up` 脉冲，必须保持可用。
- 异常 fail-closed：输入后端失败、线程 panic、规则热更新、全局关闭、应用退出时停止注入，释放应用确认由自身模拟按下的键。
- 自循环不可发生：SendInput / Interception 依赖 `SIM_MARKER`，DD-HID 依赖 pending 队列；模拟事件不得触发规则启动、停止或级联。
- 性能优化不得进入 hook 阻塞区：hook 回调不做驱动调用、文件 I/O、网络 I/O、长时间锁等待或全量规则复制。
- 用户正常操作优先：异键规则或 Toggle 规则与用户真实按住的目标键冲突时，优先保护真实输入；不内置具体按键用途判断，目标键语义由用户自行决定。
- 高精度调度必须可回退：Windows 高精度 timer、命令唤醒事件或等待调用不可用时，自动回退当前标准等待路径，不影响功能可用性。
- 调度模式默认标准等待：高精度 timer 作为设置面板中的显式选项启用，便于对比 10ms 场景收益。

## 语义边界

| 场景 | 目标行为 |
| ---- | -------- |
| `Hold trigger == target` | 正常连发，按住触发键时输出周期性目标键脉冲；松开立即停止并补释放 |
| `Hold trigger != target` | 触发键按住期间连发目标键；若用户真实按住目标键，后续安全模型应暂停或延后自动注入 |
| `Toggle target` 被用户真实按住 | 后续安全模型应暂停或延后目标键自动注入，避免打断真实操作 |
| DD-HID 无法精确标记模拟事件 | 继续使用 pending 队列；不确定来源时宁可停止相关自动行为，不级联触发 |
| 用户选择标准等待 | scheduler 只使用 `mpsc::recv_timeout`，作为默认行为和性能对照基线 |
| 用户选择高精度 timer | Windows scheduler 优先使用高精度 waitable timer；不可用时自动回退标准等待 |

## 分阶段任务

### Phase 1：低风险热路径优化

- [x] 使用规则快照替代每次按键复制 `Vec<BurstRule>`。
- [x] 编译 `trigger_key` / `stop_key` / Hold release 索引，按键事件只遍历命中的规则。
- [x] 保持现有每活动规则一线程模型，避免一次性改动调度语义。
- [x] 为同键 Hold、Toggle、重复 keydown、鼠标按键去重补齐测试。

### Phase 2：正常操作保护

- [x] 引入物理按下状态与模拟按下状态分离。
- [x] 异键目标与用户真实按住目标冲突时，自动注入让路。
- [x] 目标键用途不做产品侧判定，只保留真实输入冲突保护。
- [x] 全局关闭、规则替换、输入模式切换、退出时统一走 release ledger 兜底。

### Phase 3：单调度器与批量注入

- [x] 把每规则线程替换为单 scheduler 线程，使用绝对 deadline 管理 `down/up`。
- [x] 多规则同一 deadline 到期时批量发送 SendInput。
- [x] 同一目标键的重叠脉冲合并，避免规则互相提前抬键。
- [x] 错过 deadline 时跳过追赶，不补偿风暴。

### Phase 4：可观测与压测

- [x] 记录活动规则数、注入速率、调度延迟 p50 / p95 / p99。
- [x] 建立 Windows release 版压测脚本：1 / 8 / 32 / 64 条规则，10ms / 30ms / 50ms 间隔。
- [x] 输出 CPU、线程数、hook 回调耗时、停止响应时间。

### Phase 5：10ms 高精度调度

- [x] Windows scheduler 优先使用 `CreateWaitableTimerExW(CREATE_WAITABLE_TIMER_HIGH_RESOLUTION)` 创建高精度 waitable timer。
- [x] scheduler 命令发送后设置独立自动复位 event，调度线程用 `WaitForMultipleObjects` 同时等待命令和 timer。
- [x] timer 创建、设置或等待失败时降级到现有 `mpsc::recv_timeout` 路径，保持连发功能可用。
- [x] 高精度等待只存在于 scheduler 线程，不进入低级 hook 回调，不改变输入注入后端。
- [x] 设置面板提供标准等待 / 高精度 timer 切换，默认标准等待并持久化用户选择。

可观测入口：

- Tauri command：`get_engine_metrics`，返回活动规则数、注入总数、注入速率、scheduler 延迟、hook 回调耗时、停止响应耗时。
- Windows 进程采样脚本：`scripts/burst-engine-stress.ps1`，输出 CSV，包含 CPU、线程数、内存和句柄数。

压测矩阵建议：

| 规则数 | 间隔 | 运行方式 |
| ------ | ---- | -------- |
| 1 / 8 / 32 / 64 | 10ms / 30ms / 50ms | Windows release 包，启动全局开关后运行 `scripts/burst-engine-stress.ps1 -DurationSeconds 60` |

## 性能提升预估

以下为静态预估，最终以 Windows release 版压测为准。

| 阶段 | 主要收益点 | 预估提升 |
| ---- | ---------- | -------- |
| Phase 1 | hook 热路径从"复制全部规则 + 全量遍历"改为"克隆快照指针 + 命中索引遍历" | hook 规则匹配 CPU / 分配开销下降约 60%–90%；普通用户端到端体感提升约 5%–15%；64 规则极限输入场景 CPU 下降约 15%–35% |
| Phase 2 | 冲突场景减少无意义自动注入，异常路径统一释放 | 稳定性提升为主；冲突配置下无效注入下降约 20%–80% |
| Phase 3 | 活动规则从 N 线程改为单 scheduler，批量注入 | 32–64 条 10ms 场景线程调度开销下降约 70%–95%；整体 CPU 下降约 30%–60%；停止响应更稳定 |
| Phase 4 | 指标化后按瓶颈继续收敛 | 不承诺固定百分比，用实测决定后续是否继续调优 timer 精度 |
| Phase 5 | 高精度 waitable timer 降低 10ms 场景 scheduler 唤醒抖动 | 10ms 场景 `delay_p95_us` / `delay_p99_us` 预期下降约 30%–70%；CPU 以持平为目标，不通过忙等换精度 |

## 验收标准

- 物理输入 hook 路径全部继续传递给系统。
- `Hold trigger == target` 仍可正常输出周期性脉冲。
- 全局关闭、规则热更新、输入模式切换和应用退出后活动规则为空。
- 模拟事件不会触发其他规则启动、停止或级联。
- 64 条规则、10ms 间隔压力测试下无卡键、无线程失控、全局停止立即生效。
- 高精度 timer 不可用时自动回退标准等待路径，仍可启动、停止和释放全部活动规则。
