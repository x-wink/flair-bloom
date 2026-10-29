# 窗口绑定 — 改造路线图 / Change Roadmap

Status: active（设计已定，待开工）
Goal: 配置绑定一个游戏后，只在游戏窗口处于前台时连发；切走自动停、切回自动接着跑；游戏重启、换电脑导入同一份 `.qzh` 都不用重新绑定。

---

## 1. 背景 / Background

亲友反馈：开着连发切到浏览器或聊天窗口，连发的键会打进去；希望连发只对游戏生效。现在引擎不看前台窗口，只认全局开关（`packages/burst-engine/src/lib.rs:356`）。

| 能力 | 现状 |
| --- | --- |
| 前台窗口检测 | 无，仓里没有 `GetForegroundWindow` / WinEvent 相关代码 |
| 「挡住规则启动、不动全局开关」的闸门 | 有先例：`switching_backend`（`lib.rs:172`），新闸门照这个模式加 |
| 停掉全部连发并释放按键 | 有：`stop_runtime_activity`（`lib.rs:683`），全局暂停、换规则、切后端共用 |
| 「暂停不改变活跃集合、不触发播报」 | 有：插队暂停的 `paused` 口径（`lib.rs:286`） |
| 配置里存一个前端不负责的字段 | 有先例：水印署名走 `carry_over_meta` 加专用命令（`commands/profile.rs:139`、`:184`） |
| 设置 → 配置文件一栏 | 有：署名栏所在页，绑定窗口放在旁边 |

不可破坏的不变量：

- 停止键与全局热键 100% 生效，不受新闸门影响（`lib.rs:328`、`:347`）。
- 闸门关闭时目标键一律释放，不留卡键；钩子线程不阻塞（`lib.rs:413` 的注释）。
- 不绑定的配置行为与现在完全一致；旧配置读进来就是「不绑定」。

## 2. 决策 / Decisions

- **D1 按 exe 文件名匹配，窗口类名兜底，标题关键字可选。** 理由：HWND、PID 每次启动都变，存了就要每次重绑；完整路径在游戏换目录或按版本号更新目录后会变。exe 文件名不区分大小写比较。AutoHotkey 的 `ahk_exe`、G Hub / Synapse 的「关联程序」都按 exe 认程序。
- **D2 绑定跟随配置**，`Profile` 新增 `#[serde(default)] window_binding: Option<WindowBinding>`，**不升 `CURRENT_SCHEMA_VERSION`**。理由：一个游戏一份配置本就一一对应；`.qzh` 分享给别人时绑定也跟着走；本仓禁止裸 bump schema。
- **D3 自家进程（面板、浮窗、对话框）在前台时算「匹配」**，闸门打开。理由：在面板里试规则、走新手教程（groups 第 4～6 步）要求连发能跑。判断方法是比较前台窗口的 PID 与 `GetCurrentProcessId()`，不需要逐个记窗口句柄。
- **D4 切走只停连发，不改全局开关**：不刷新托盘图标，不广播 `global-enabled-changed`，不触发语音。理由：Alt+Tab 很频繁，每次都翻开关会吵，也会和用户自己的开关操作混在一起。
- **D5 切回自动恢复切换（toggle）规则**（作者选定）。切走时记下正在跑或被插队暂停的切换规则，切回时重新启动；长按规则必须重新按住。理由：手感顺；长按无法恢复，因为切走期间按键状态已经清空。
- **D6 挂起的规则沿用 `paused` 口径**：仍算在 `get_active_ids` 里，界面显示为暂停。理由：前端靠活跃集合的差分来播报，集合不变，切走切回就不会播报「停止 / 开始」；与插队暂停的语义一致。
- **D7 挂起集合在这些情况下作废**：换规则（`set_rules`，含自动保存）、全局关闭、切后端、退出。理由：规则 ID 可能已不存在；用户在别处关了全局开关，回到游戏不该自己开打。
- **D8 前台检测用 `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`，事件驱动、不轮询**，在独立线程上跑，新建 crate `win-foreground`。理由：轮询要么延迟高要么费 CPU；回调里还要做 `OpenProcess`，不能放在低级钩子线程上拖慢系统输入。
- **D9 绑定后界面常驻明显标识**（作者要求）：只要当前配置有绑定，面板顶栏和浮窗胶囊就一直显示「仅 xxx.exe」标签，不是只在闸门关闭时才出现。闸门关闭时标签变灰，同时显示「等待 xxx.exe」。点标签直达设置 → 配置文件的绑定栏。配置列表里有绑定的配置带一个标记，托盘提示文字也带上绑定的 exe 名。理由：绑定后最容易让人困惑的情况是「按了没反应」，用户得一眼看出是绑定在起作用，而不是以为坏了。

- **D10 绑定窗口是第二项亲友专属权益，权益键 `flair.window-binding`**（作者定，2026-10-29）。没有这项权益时绑定不生效：闸门始终打开，界面不显示绑定栏和标识，文件里的绑定字段原样保留。别人分享来的带绑定的 `.qzh` 也一样处理，和署名的口径一致（license 路线图 D10）。权益在运行中到期时，下一次计算闸门就按「无权益」处理，把闸门打开。理由：作者定位为亲友专属；保留字段是为了续期或重新激活后绑定还在，不用重绑。
- **D11 更新公告写进 `### 亲友专属` 一节**（license 路线图 D24）；新手教程不讲，和署名一样。

## 3. 问题清单 / Findings

| # | 问题 | 严重度 | 位置 | → 任务卡 |
| --- | --- | --- | --- | --- |
| I1 | 前端自动保存每次从头拼 `Profile`，不会带上新字段，按 `serde(default)` 落成 `None`，**每次改规则都会清掉绑定** | P0 | `commands/profile.rs:146-168` | T2 |
| I2 | 闸门关闭后停连发会清空 `physical_pressed`；恢复切换规则不能走 `handle_rule_press`（它要求物理键是新按下），需要一条直接启动的路径，并且要遵守组互斥和插队栈 | P1 | `lib.rs:683-717`、`:365-368` | T4 |
| I3 | 挂起的规则如果移出活跃集合，前端差分会把切走、切回播报成「停止 / 开始」 | P1 | `lib.rs:286`、`panel/useRuleStates.ts` | T4 |
| I4 | 前台事件只在切换时触发：启动时、换配置时游戏已经在前台，闸门不会自己算出来 | P1 | `commands/profile.rs:233` | T5 |
| I5 | 前台回调线程若同步等待停止完成会卡住；停止必须用非阻塞方式（`wait=false`） | P1 | `lib.rs:396-421` | T4、T5 |
| I6 | 反作弊可能剥掉外部进程对游戏的查询权限，`QueryFullProcessImageNameW` 失败，拿不到 exe 名 | P2 | `win-foreground`（新建） | T3、T8 |
| I7 | 从其他工具导入、新建配置时用结构体字面量构造 `Profile`，加字段后要补齐 | P2 | `commands/import_profile.rs:398`、`commands/profile.rs:393` | T1 |

## 4. 任务卡 / Task cards

### T1 — 绑定的数据结构与匹配逻辑 · 进度: 未开始

- **Fixes:** I7
- **Change:** `packages/qzh-profile` 新增 `WindowBinding { exe, class, title_contains }`（后两项为 `Option`）与纯函数 `matches(binding, &ForegroundInfo) -> bool`：能拿到 exe 名时要求 exe 匹配，设了类名就同时比类名，设了标题关键字就要求标题包含它；拿不到 exe 名时只靠已记录的类名兜底，没有记录类名就判为不匹配。`sanitize_profile` 对各字段截断长度、去掉控制字符。补齐 I7 里的两处结构体字面量。
- **Acceptance:** 老配置反序列化后 `window_binding == None`；带绑定的配置经过 序列化 → 反序列化 后不变。
- **Test:** `cargo test -p qzh-profile`：大小写、exe 名缺失时用类名兜底、标题包含、空字段、超长字段的截断。
- **Rollback:** 字段是 `serde(default)`，回退代码后旧版本读到这个字段会忽略，不砸配置。
- **Depends on:** 无

### T2 — 保存时保留绑定，专用写入命令 · 进度: 未开始

- **Fixes:** I1
- **Change:** `save_profile` 写盘前从旧文件带上 `window_binding`（与署名同一思路，`carry_over_meta` 扩成按整份旧配置带过去）；新增命令 `set_profile_window_binding(name, binding: Option<WindowBinding>)`，写完后如果改的是当前配置，就通知 T5 的闸门重新计算。
- **Acceptance:** 设了绑定之后连续改规则、自动保存多次，绑定仍在；重命名、复制配置后绑定保留。
- **Test:** `profile_tests.rs` 补「自动保存不带绑定时沿用旧值」「新文件没有绑定」两条。
- **Rollback:** 回退提交；已写进文件的绑定字段旧代码会忽略。
- **Depends on:** T1

### T3 — `win-foreground` crate：前台监听与窗口枚举 · 进度: 未开始

- **Fixes:** I6
- **Change:** 新建 crate（`[lints] workspace = true`，非 Windows 平台给空实现）。① `ForegroundWatcher`：独立线程跑消息循环，挂 `EVENT_SYSTEM_FOREGROUND`，每次切换产出 `ForegroundInfo { pid, exe, class, title }`（`exe` 用 `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` 加 `QueryFullProcessImageNameW` 取文件名，失败时为 `None`）；另提供 `current()` 供启动时取一次。② `list_windows()`：`EnumWindows` 只列可见、有标题、不是工具窗口的顶层窗口，排除自家 PID，给绑定界面用。
- **Acceptance:** 示例程序在前台来回切换时打印的 exe 名与类名正确；以管理员身份运行的程序也能拿到 exe 名。
- **Test:** `examples/foreground_smoke.rs` 真机冒烟（需要交互式桌面，同 `testing.md` 里 `#[ignore]` 的口径）。
- **Rollback:** 新 crate 不被引用即无影响。
- **Depends on:** T1（`ForegroundInfo` 与匹配逻辑放在 qzh-profile，这里只负责产出数据）

### T4 — 引擎：前台闸门、挂起与恢复 · 进度: 未开始

- **Fixes:** I2、I3、I5
- **Change:** `BurstEngine` 加 `foreground_ok: AtomicBool`（默认为真）和 `suspended: Vec<String>`（存在 runtime 锁里）。`on_key_press_event` 在 `switching_backend` 判断旁边加一道 `foreground_ok` 检查（热键与停止键在它之前处理，不受影响）。新增 `set_foreground_ok(bool)`：关闭时在锁内记下活跃切换规则的 ID，再走 `stop_runtime_activity(false)`；打开时如果全局开着，就按记下的顺序直接启动这些规则（经过组互斥逻辑，不碰 `physical_pressed`）。按 D6，挂起的规则计入 `get_active_ids` 并归到 `get_rule_states.paused`。按 D7，`set_rules`、全局关闭、切后端、退出时清空 `suspended`。
- **Acceptance:** 闸门关闭时没有任何目标键处于按下状态；打开后切换规则恢复运行、长按规则不恢复；挂起期间关掉全局开关，再打开闸门也不会开打。
- **Test:** `burst-engine` 单测（用命令录制替身调度器）：关闭 → 停止全部；打开 → 只启动记下的切换规则；与插队栈组合（长按插队时切走，切回只恢复被暂停的切换规则）；D7 四种作废情况；活跃集合在关闭、打开前后保持不变。
- **Rollback:** `foreground_ok` 恒为真时等价于现状；回退提交即可。
- **Depends on:** 无（可与 T1～T3 并行）

### T5 — 接线：配置激活、闸门计算、事件 · 进度: 未开始

- **Fixes:** I4、I5
- **Change:** app 层持有 `ForegroundWatcher` 与当前配置的 `Option<WindowBinding>`。`activate_profile_file` 与 T2 的命令更新绑定后，以及激活兑换码之后，都立即用 `current()` 算一次闸门；监听回调里按「无权益 → 打开；自家 PID → 匹配；无绑定 → 匹配；否则调 `matches`」得出结果（权益每次现查 `LicenseState::grants`，切前台不频繁，开销可以忽略），有变化才调用 `engine.set_foreground_ok` 并广播 `foreground-gate-changed { open, exe }`。回调只做计算和入队，不同步等待停止完成。
- **Acceptance:** 游戏已在前台时启动应用或切换配置，连发立即可用；切到浏览器后停止，切回游戏后恢复；切到面板时连发照常可用。
- **Test:** 判定函数（自家 PID / 无绑定 / 匹配）抽成纯函数写单测；联调放在 T8。
- **Rollback:** 不初始化监听线程就退回现状。
- **Depends on:** T2、T3、T4

### T6 — 界面：绑定栏与等待状态 · 进度: 未开始

- **Change:** `crypto` 与前端 `license.ts` 加 `ENT_WINDOW_BINDING = 'flair.window-binding'`，在 `ENTITLEMENTS` 里登记说明和「去绑定」按钮（直达设置 → 配置文件）；`set_profile_window_binding` 与署名一样先校验权益。有权益时，设置 → 配置文件在署名栏旁边加「绑定窗口」：点「选择窗口」弹出 `list_windows` 列表（标题 + exe 名），选中后记下 exe 名与类名；可选填标题关键字；「解除绑定」。改「默认配置」时与署名一样先另存为新配置（`ensureWritableProfile`）。按 D9 做常驻标识：面板顶栏（竖版、横版）与浮窗胶囊显示「仅 xxx.exe」标签，闸门关闭时变灰并显示「等待 xxx.exe」，点击直达绑定栏；配置列表与托盘菜单的配置项带绑定标记；托盘提示文字带 exe 名。
- **Acceptance:** 有绑定的配置在面板、浮窗、配置列表、托盘四处都能看出来；切到别的窗口时标签在 1 秒内变成「等待」；亮暗两种主题、20 套预设主题色下标签文字都过 4.5:1（沿用 `contrast.ts`）；横版、竖版、浮窗三处都不挤压原有布局。
- **Test:** 浏览器 mock 截图核对。
- **Rollback:** 隐藏这一栏即可，后端没有绑定就不生效。
- **Depends on:** T2、T5

### T7 — 文档同步 · 进度: 未开始

- **Change:** `manual/profiles.md` 的「兑换码与水印署名」一节扩成「兑换码与亲友专属权益」，补上绑定窗口（含常见坑：反作弊游戏拿不到 exe 名时靠类名、UWP 游戏暂不支持；没有权益的人看不到这一栏），教程不加步骤；`engineering/architecture.md` 的引擎段补前台闸门与挂起口径，`engineering/profile-format.md` 补 `window_binding` 字段；`README.md`「接下来」删掉这一条；`CHANGELOG.md` `[Unreleased]` 的 `### 亲友专属` 节。`pnpm skills:check` 通过。
- **Depends on:** T6

### T8 — 真机冒烟 · 进度: 未开始

- **Fixes:** I6
- **Change:** 按 `manual/profiles.md` 在测试配置里操作：绑一个真实游戏 → 开一条切换规则 → Alt+Tab 到浏览器（停止、键不卡）→ 切回（恢复）→ **退出游戏再启动**（不用重绑也能生效）→ 导出 `.qzh` 再导入（绑定还在）。分别用管理员身份运行的游戏、带反作弊的游戏各过一遍，记录 exe 名能否拿到。
- **Depends on:** T5、T6

## 5. 依赖与顺序 / Dependencies & order

```
T1 ─> T2 ─┐
T1 ─> T3 ─┼─> T5 ─> T6 ─> T7
      T4 ─┘            └──> T8
```

- **执行顺序：** T1 → T4 → T2 → T3 → T5 → T6 → T7 → T8（T4 与 T1～T3 无依赖，可以先做）。
- **最小可交付里程碑：** T1～T5，加上最简的绑定栏（只有选择窗口和解除绑定）和面板、浮窗上的常驻标签（D9 不能砍：没有标识的绑定会被当成故障）。配置列表与托盘的标记可以放到后面再做。

## 6. 工期 / Effort

口径：作者加 AI 单线推进，单位为净工作日；校准来源是 `archive/onboarding-tour.md`（预估 6.5 天，逐卡 ✅，没有偏差）和 `archive/hold-interrupt.md`（引擎状态机加单测估 1 天）。

| 任务卡 | 预估 | 实际 | 偏差说明 |
| --- | --- | --- | --- |
| T1 | 0.5 | | |
| T2 | 0.5 | | |
| T3 | 1 | | |
| T4 | 1 | | |
| T5 | 0.5 | | |
| T6 | 1.5 | | |
| T7 | 0.5 | | |
| T8 | 0.5 | | |
| **合计** | 6 | | |

最大不确定项是 T8：要准备有反作弊的真实游戏，如果拿不到 exe 名的情况普遍，T3 的类名兜底可能要加强（例如加上窗口标题），可能多出 0.5 天。

## 7. 风险与回滚 / Risks & rollback

- **R1 反作弊挡住进程查询：** 类名兜底；仍然认不出时界面提示改用标题关键字。T8 实测后决定要不要再加一层兜底。
- **R2 恢复时启动的规则与当下按住的键冲突：** 例如切回时手正按着同组的长按键。按住状态在切走时已经清空，恢复只按组互斥启动，长按要重新按住才会插队。T4 的单测覆盖这个组合。
- **R3 前台事件丢失或乱序：** 每次回调都直接查当前前台窗口来判断，不依赖事件里带的窗口句柄；启动和换配置时各主动算一次。
- **回滚：** 闸门默认打开，绑定为 `None` 时与现状一致；整组提交回退后，已写进文件的字段旧版本会忽略。

## 外部依赖

- billing（xwink-console）需要在生产环境登记 `flair.window-binding` 并建兑换码批次，每次写生产都要经作者点头（同 license 路线图 T4）。这一步拉长日历时间，但不占本路线图的净工时；开发期用开发私钥签的测试许可证验证。

## 8. 范围外 / Out of scope · follow-ups

- 按前台游戏自动切配置（README「接下来」）：复用 T3 的监听与 T1 的匹配逻辑，另开一份路线图。
- UWP / Game Pass 游戏：前台窗口属于 `ApplicationFrameHost.exe`，需要再找它的子窗口才能认出游戏，先在说明书的常见坑里注明不支持。
- 一份配置绑定多个窗口：等有人提出需求再说，`WindowBinding` 届时改成列表要升 schema。
- 绑定列表里显示窗口图标。
