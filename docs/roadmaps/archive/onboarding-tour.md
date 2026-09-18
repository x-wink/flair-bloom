# 新手引导教程 + 仓内专属 skill — 改造路线图

Status: completed
Goal: 新用户不看文档、5 分钟内在气泡引导下配好并跑起第一条规则；使用说明与架构事实各只有一份源，README / CLAUDE.md / 应用内教程都从它派生，改一处不漂。

---

## 0. 最终形态

做完之后用户、代码、文档三处各长什么样。这一节是验收对照物，任务卡只是拆它。

### 0.1 用户看到的

**☰ 菜单**多一项「新手教程」（位于「主题颜色」与「检查更新」之间），点开是教程目录：

```
┌──────────────────────────────────────────┐
│ 新手教程                                  │
│ 每组几分钟，随时可退出，学过的会打勾        │
├──────────────────────────────────────────┤
│ ✓ 上手三步            5 步    [重新学习]   │
│   添加第一条规则并让它跑起来               │
│ ○ 游戏模式与驱动       4 步    [开始]      │
│   为什么游戏里要装驱动、怎么装             │
│ ○ 规则玩法            5 步    [开始]      │
│   按压 / 切换、高级设置、互斥分组          │
│ ○ 横版键鼠图与浮窗     4 步    [开始]      │
│   （DD驱动下显示「当前输入模式不支持」）    │
│ ○ 多套配置            4 步    [开始]      │
│ ○ 热键、声音与外观     5 步    [开始]      │
├──────────────────────────────────────────┤
│                                  [关闭]   │
└──────────────────────────────────────────┘
```

**引导气泡**：界面变暗，只有当前讲的控件亮着（主题色描边），气泡贴在旁边、带箭头，跟随亮暗与门派色：

```
┌────────────────────────────────────────────┐
│ 🌸 气质花按键助手        ▭   ☰   —   ✕      │ ← 变暗
├────────────────────────────────────────────┤
│   按压连发 0/0    │   切换连发 0/0          │ ← 变暗
├────────────────────────────────────────────┤
│                                            │
│ ╔══════════════════════════════════════╗   │ ← 主题色描边、原亮度
│ ║       + 添加按压连发规则              ║   │
│ ╚══════════════════════════════════════╝   │
│         ▲                                  │
│ ┌───────┴────────────────────────────┐     │
│ │ 添加第一条规则               2 / 5 ✕│     │
│ │ 点这个按钮，下面会出现一张规则卡。    │     │
│ │ ● 等你点一下…                        │     │ ← 实操步骤：做完自动到下一步
│ │              [跳过这步]  [上一步]     │     │
│ └────────────────────────────────────┘     │
│                                            │
├────────────────────────────────────────────┤
│  默认配置 ▾     游戏模式 ▾    [全局已禁用]  │ ← 变暗
└────────────────────────────────────────────┘
```

- 讲解步骤：气泡底部是「上一步 / 下一步」（最后一步是「完成」）；实操步骤：亮着的控件可以点、其余地方点不动，做完的一瞬间气泡打勾并自动前进，也可「跳过这步」。
- 右上 ✕ 或 Esc 随时退出整组；退出不算完成。走到「完成」才在目录里打勾。
- 教程要讲设置页时自己打开设置弹窗、切到对应页签；气泡叠在弹窗之上。退出教程时弹窗留在原地，不替用户关。
- 协议弹窗、更新就绪弹窗出现时气泡先让路，弹窗关掉后回到同一步。
- 上手三步的 3 个实操：点「+」添加规则 → 在按键框按下一个键 → 点亮「全局已禁用」。走完时面板上真的多了一条在跑的规则。

**首次启动**：同意协议后，如果配置里一条规则都没有，直接进入「上手三步」；有规则的老用户升级后什么都不弹，只在菜单里多了入口。

**settings.json** 多一个键：`"tours": { "completed": ["getting-started"], "introShown": true }`。

### 0.2 代码

```
apps/main/src/windows/panel/
  tour/
    types.ts            TourDef / TourStep / TourHost / TourSnapshot
    TourRunner.tsx      遮罩 + 高亮 + 气泡 + 步骤推进（唯一有 DOM 逻辑的文件）
    TourRunner.css
    useTourProgress.ts  settings.json 的 tours 键读写
    tours/
      index.ts          有序导出六组（id 即文件名，供文档校验脚本比对）
      getting-started.ts
      game-mode.ts
      rules.ts
      layouts.ts
      profiles.ts
      settings.ts
  dialogs/
    TourCatalogDialog.tsx / .css   教程目录
  components/Overlay.tsx           导出 computePos / clampPos；zIndex 改读 Token
  theme.css                        + --fb-z-tour-mask / --fb-z-tour-bubble
  PanelApp.tsx                     组装 TourHost、挂 TourRunner、菜单项、首启触发、data-tour 锚点
  HorizontalLayout.tsx / dialogs/SettingsDialog.tsx   data-tour 锚点
```

一组教程长这样（示意，不是最终文案）：

```ts
export const gettingStarted: TourDef = {
  id: 'getting-started',
  title: '上手三步',
  summary: '添加第一条规则并让它跑起来',
  steps: [
    { id: 'overview', target: 'tabs', title: '两种连发', body: '按压连发：按住就连、松手就停…' },
    {
      id: 'add', target: 'add-hold', title: '添加第一条规则',
      body: '点这个按钮，下面会出现一张规则卡。',
      prepare: (host) => host.setActiveTab('hold'),
      done: (now, entered) => now.rules.length > entered.rules.length,
    },
    { id: 'key', target: 'rule-key', title: '按下要连的键', body: '点一下按键框，再按键盘或鼠标上的键。',
      done: (now, entered) => !keyEq(lastRule(now).target_key, lastRule(entered).target_key) },
    …
  ],
};
```

### 0.3 文档

```
CLAUDE.md                 ≤ 80 行：项目一句话、常用命令、「按任务读取」索引表、硬约束一览
README.md                 首屏（宣发 / 三步上手 / 玩游戏必看 / 常见问题 / 风险声明）+ 玩法索引表 + 「接下来」计划列表
skills/flair-bloom/
  SKILL.md                索引：按任务读取哪篇 reference；事实边界
  references/
    manual/               面向用户，与六组教程一一对应
      getting-started.md  game-mode.md  rules.md  layouts.md  profiles.md  settings.md
    engineering/          面向开发，CLAUDE.md「关键架构决策」+ ROADMAP 现状章节迁入、去重
      architecture.md     进程模型、多窗口、事件通信、输入模式与注入通道、AppHandle 规则、存储路径
      key-policy.md       按键角色与录入策略、判定与提醒两层、强制点
      profile-format.md   .qzh 结构、schema 迁移链、输入约束表
      updater.md          自动更新、公告两个来源、Markdown 渲染
      release.md          发版步骤、授权口径、护栏、回滚
      development.md      目录结构、命令、协作规范、lint / 覆盖率门槛、设计准则
docs/
  roadmaps/
    onboarding-tour.md    本文件（完成后移到 archive/）
    pet-mode.md           Status: draft，原 ROADMAP「桌宠模式设计」改写
    license.md            Status: draft，原「许可证系统」+「亲友专属功能」改写
    archive/
  ASSETS.md  TESTING.md  WINDOWS_VERIFY.md  RELEASE_ROLLBACK.md  …   不动
  ROADMAP.md              删除
scripts/check-skills.ts   tours/index.ts 的 id ⇔ manual/*.md 一一对应；SKILL.md / CLAUDE.md / README.md 相对链接都存在
.githooks/pre-commit      暂存 .md 或 tour/ 下文件时跑 pnpm skills:check
.claude/skills/           三条软链改指向 .agents/skills/<同名>（相对路径）；skills/flair-bloom 不软链
```

`CLAUDE.md` 的索引表形态：

```
| 任务                         | 读取                                                  |
| ---------------------------- | ----------------------------------------------------- |
| 改窗口、事件、输入模式、注入 | skills/flair-bloom/references/engineering/architecture.md |
| 改按键录入、热键、槽位策略   | …/key-policy.md                                       |
| 改 .qzh、schema、约束        | …/profile-format.md                                   |
| 改更新、公告、Markdown       | …/updater.md                                          |
| 发版                         | …/release.md（推 tag 前一次一授）                      |
| 改用户可见行为、写说明        | …/manual/<主题>.md，同时改 tour/tours/<主题>.ts        |
```

## 1. 背景

### 被改对象

- **面板前端** `apps/main/src/windows/panel/`：单窗口 React 应用，竖版 405×720 / 横版 1060×580。已有 `components/Overlay.tsx` 提供 12 方位锚定 + 视口钳位 + 遮罩；设计 Token（`theme.css`）齐全，亮暗与 21 套门派色都靠变量；`settings.json`（`tauri-plugin-store`）是现成的持久化位置。
- **文档**：`README.md`（面向用户的说明书，约 120 行）、`CLAUDE.md`（约 200 行，一半是「关键架构决策」正文）、`docs/ROADMAP.md`（战略路线图）、`apps/site/app/utils/content.ts`（产品页与打印说明书的文案副本）。同一件事（如「游戏模式要装驱动并重启」）当前在 README、CLAUDE.md、ROADMAP 当前阶段、site content.ts 四处各写一遍。

### 参考实现

- **Clash Party**（`mihomo-party-org/clash-party`，Electron）：`driver.js` 单例，`driver({ showProgress, nextBtnText, overlayOpacity, steps: [{ element, popover: { title, description, side, align, onNextClick } }] })`；跨页步骤靠 `onNextClick` 里先 `navigate()` 再 `setTimeout(moveNext, 0)`；首启用 `localStorage.tourShown` 判一次。可借鉴的是气泡 + 高亮镂空 + 进度 + 上一步/下一步/跳过的交互形态；不借鉴 driver.js 本身（见 D1）。
- **VS Code Walkthroughs**：多组教程按主题并列，每组有标题、简介、步骤清单与完成勾选，可反复进入。借鉴的是「教程目录 + 每组独立完成状态 + 关键步骤要真做」的组织方式。
- **xwink-console**：仓库根 `skills/xwink-engineering/SKILL.md` 做索引，`references/*.md` 每个主题一篇；`AGENTS.md` 只留「按任务读取」索引表；`README.md` 短入口 + 链接；`pnpm skills:check` 校验 owner 文档结构。借鉴的是「skill 即文档单一来源、入口文件只做索引」。

### 现状盘点

| 能力           | 现状                                                                                                                                                                              |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 锚定气泡定位   | 已有：`Overlay.tsx` `computePos` 支持 12 方位 + `clampPos` 视口钳位；但只在 `version` 变化时重算一次，不跟随目标滚动/窗口尺寸变化（`Overlay.tsx:321-361`）                        |
| 遮罩层         | 已有：`.overlay-mask` 整块遮罩；无「镂空高亮」形态                                                                                                                                |
| 层级 Token     | 已有 `--fb-z-overlay-mask: 99990` / `--fb-z-overlay-content: 99999`；`Overlay.tsx` 里 `zIndex: 99999` 是硬编码而非取 Token（`Overlay.tsx:316,332,343`）；无高于设置弹窗的引导层级 |
| 稳定的界面锚点 | 无：标题栏五个按钮同为 `.win-btn`，规则卡片、底部三按钮、设置页签都只有语义类名，无 `data-tour` 之类的引导锚点                                                                    |
| 首启判定       | 无：唯一首次信号是 `needs_agreement`（`PanelApp.tsx:800`），协议一旦同意就再无「新用户」标记                                                                                      |
| 教程持久化     | 无：settings.json 有 `closeBehavior` / `activeTab` / `sound` / `theme` / `layout` / `autoUpdate`，新增键是加法，不涉及 schema 版本                                                |
| 宿主可编程控制 | 部分：`setActiveTab` / `switchLayout` / `handleShowSettings(tab)` / `addRule(mode)` 都是 `PanelApp` 内部函数，可直接交给引导层调用                                                |
| 教程入口       | 无：☰ 菜单项为 设置 / 主题颜色 / 检查更新 / 更新公告 / 用户协议 / 诊断修复 / 关于（`PanelApp.tsx:2730-2792`）                                                                    |
| 仓内 skill     | 无：`.agents/skills/` 只有三份第三方最佳实践；`.claude/skills/*` 三条软链指向已改名的旧目录 `D:/Workspace/burtst`，全部失效                                                       |
| 文档结构校验   | 无：pre-commit 只跑 `cargo fmt/clippy` 与 `oxlint/oxfmt`；CI 只有 Rust 门禁与覆盖率，无前端/文档校验                                                                              |
| 战略路线图条目 | `docs/ROADMAP.md:809-811` v0.3「首次引导」两项均 `[ ]`：协议同意后引导流程；内置规则模板（后者本轮不做，见第 8 节）                                                               |

### 不可破坏的不变量

- 引导层不得吞掉引擎需要的按键事件：面板聚焦时键盘事件经 `useKeyRelay` 中继到后端，教程「按下要连的键」这一步依赖 `KeyCapture` 正常聚焦录入。
- 引导打开期间用户协议 / 更新弹窗仍可正常出现并优先；引导不阻塞任何既有流程，随时可 Esc / 跳过。
- 文档收口后，CLAUDE.md 里现有的所有架构事实必须能在 skill 里逐条找到，不丢内容。

## 2. 决策

- **D1：引导气泡自研，复用现有 Overlay 定位算法与 Token，不引入 driver.js** — 理由：面板只有 405px 宽，driver.js 的默认遮罩与气泡样式要整套覆写才能跟随 21 套门派色与亮暗；它是全局单例、不感知 React 状态，跨弹窗 / 切布局的步骤只能靠 `setTimeout` 等 DOM，而本仓的 `setActiveTab` / `handleShowSettings` 就在同一组件里，直接调用更可靠。
- **D2：仓内 skill 同时收口使用说明与架构文档** — 理由：用户明确要求；README / CLAUDE.md / site 文案四处重复已是现实，只收使用说明那一层仍留一半漂移面。
- **D3：教程是「讲解 + 关键步骤等用户实操」** — 理由：纯讲解看完仍不会；VS Code walkthrough 的完成勾选来自真做过。实操步骤限于三个：点「+」添加规则、在按键框里按下一个键、点亮全局开关；其余步骤只高亮讲解。
- **D4：skill 落在仓库根 `skills/flair-bloom/`，一个 skill 两组 reference** — `references/manual/`（面向用户：六个教程主题各一篇）与 `references/engineering/`（面向开发：架构 / 按键策略 / 配置格式 / 更新 / 发版 / 开发环境）。理由：仓库只有一个产品，不需要 xwink-console 的「根工程 skill + 每 app owner skill」两层；但用户说明与架构事实读者不同，目录分开便于按任务读取。
- **D5：教程步骤文案的源是 TS 定义（`tour/tours/<id>.ts`），reference 只是大纲** — 理由：步骤需要锚点、准备动作、完成判定，Markdown 表达不了；反过来让 `scripts/check-skills.ts` 校验「每个教程 id 都有对应 `references/manual/<id>.md`，README 索引链接的每个文件都存在」，把大纲与实现钉在一起。
- **D6：引导层用自己的 portal 与两档新 Token（`--fb-z-tour-mask` / `--fb-z-tour-bubble`），高于 `--fb-z-overlay-*`** — 理由：设置弹窗是 `Overlay` 带遮罩（z 99990），教程要在它之上讲页签；遮罩用「上下左右四块」而非整块 + 镂空，非实操步骤四块都拦点击、实操步骤放开镂空区，无需 SVG mask。
- **D7：持久化键 `settings.json` → `tours: { completed: string[], introShown: boolean }`** — 理由：与 `autoUpdate` 等同一份文件、同一套读写；纯加法不动迁移链。
- **D8：首启自动跑「上手三步」，条件是 `introShown` 未置且规则数为 0；否则只静默置位** — 理由：老用户升级后已有规则，不该被强推教程；规则为 0 是「新用户」最可靠的近似，`needs_agreement` 同意后落到同一条路径。教程随时可在 ☰ 菜单「新手教程」进入，目录里能看到每组完成状态与「重新学习」。
- **D9：横版键鼠图教程按 `coincident_toggle` 能力位决定是否可进入** — 理由：DD 驱动下横版本来就被 `switchLayout` 拦下，教程不应绕过这条互斥。
- **D10：宿主接口（`TourHost`）由 PanelApp 组装后传给引导层** — `{ snapshot(): { rules, globalEnabled, inputMode, layout, activeTab, settingsOpen, keyPolicies }, setActiveTab, setLayout, openSettings(tab), closeSettings, addRule(mode) }`。步骤的 `prepare(host)` 在展示前调用（切页签 / 开设置），`done(snapshot)` 是实操步骤的完成判定。理由：引导层不 import PanelApp 内部状态，PanelApp 也不认识具体教程，两边只隔一个接口。

- **D11：`.claude/skills` 只软链 `.agents/skills/` 下给 Claude Code 用的第三方技能；应用自己的 `skills/flair-bloom` 不进 Claude 扫描范围** — 三条失效软链改为指向 `.agents/skills/<同名>` 的相对路径。理由：产品 skill 是文档单一来源，由 CLAUDE.md 索引按需读取，不作为常驻技能注册（与 xwink-console「不注册为开发 Agent 常驻技能」一致）。
- **D12：README 首屏保留，「玩法」改索引** — 保留头部宣发、三步上手、玩游戏必看、常见问题与风险声明；「多玩点花样」整段改为指向 `references/manual/*.md` 的索引表。理由：GitHub 首屏信息量不降，重复面只留在必须一眼看到的部分。
- **D13：教程六组** — `getting-started` / `game-mode` / `rules` / `layouts` / `profiles` / `settings`。
- **D14：产品页 `apps/site/app/utils/content.ts` 文案副本本轮不动** — 记入第 8 节。理由：独立 pnpm 项目与发版链，跨仓引用 Markdown 要加构建层。
- **D15：`docs/ROADMAP.md`（981 行）降级，今后规划一律按 project-planning 标准维护** — 拆法：现状事实（架构说明 / 目录结构 / 进程模型 / 发布基础设施 / 事件通信 / 数据路径 / 参数约束 / 卸载策略 / 配置文件格式 / 许可证 / 用户协议 / 功能分层 / 技术选型 / 架构设计准则）并入 skill `references/engineering/*`，与 CLAUDE.md 同主题段落合并去重；迭代计划里未完成的 `[ ]` 项与「待定」、以及只是设想的方向，一起进 README 末尾「接下来」一节，每条一行、用户视角措辞、不分「已定」与「设想」；值得留但未批准的详细设计（桌宠模式设计、许可证系统、亲友专属功能）改写为 `docs/roadmaps/<name>.md` 并标 `Status: draft`，开工时转 active；已完成的 `[x]` 项与「当前阶段」叙述删除（git 历史与 CHANGELOG 已记录）；不建 `docs/TODO.md` / `docs/IDEAS.md`。每个改动的规划落 `docs/roadmaps/<change>.md`（Status / 决策 / 问题清单 / 任务卡 / 工期表），完成后归档到 `docs/roadmaps/archive/`。理由：一份 981 行的战略文档既装现状又装计划又装设想，三类内容更新节奏不同，混在一起既不是事实源也不可执行；「只描述现状」的文档约定要求已完成项不再叙述；计划列表放 README 是开源产品惯例，但设计细节不属于面向玩家的说明书，故用 draft 路线图承接。
- **D16：本任务收尾后回灌 wink-skills，`requirement-roadmap` 重构为 `project-planning`（已完成，见 T6），`invoice` 并入为按需加载的独立章节** — 三层规范：项目文档单一来源（仓内 skill）、README「接下来」计划列表、每改动一份路线图 + `draft / active / completed / archived` 生命周期与归档；吞并 `legacy-onboarding` 的「规划迭代」一步（后者只留勘察 / 协作规范 / 现状评估，规划指向新技能），同步改 wink-skills README 技能表、`invoice` / `session-orchestration` 的引用与 `~/.claude/skills` 软链，并在 `PROJECTS.md` 补 flair-bloom 台账。理由：规范已在 xwink-console 与本仓两个项目复现，够得上固化；不改 legacy-onboarding 则下一个老项目接入时会再造一份战略 ROADMAP；invoice 本就以路线图工时表为唯一数据源，并入后契约在同一技能内、不再跨技能维护。⚖️ 两处代价：规划与开票语义相距远，同一 description 承载两组触发短语可能降低任一方命中率，靠各写一句明确短语缓解；个人收款信息进入通用技能，靠章节级 🔒 标记与「只在开票时读取」隔离。
- **D17：多会话分工** — 指挥会话 flair-bloom-02（Fable）亲自做 T1 引导引擎、T4 文档收口与 T6 回灌；执行会话 flair-bloom-d5（Opus）做 T4-a 仓库基建（软链修复、校验脚本、pre-commit）与 T2 只读预研，T1 提交后接 T2 → T3 → T5。两边同时在工作区写但路径不相交（指挥方：`apps/main/src/windows/panel/tour/`、`Overlay.tsx`、`theme.css`、`skills/`、`CLAUDE.md`、`README.md`、`docs/`；执行方：其余 `apps/main/`、`scripts/`、`.githooks/`、`package.json`、`.claude/skills/`），各自只用 pathspec 提交自己的路径。理由：T1 定接口与视觉判断点、T4 的合并去重、T6 的技能措辞都需要指挥方持有的全部上下文；T2/T3 是接口既定后的照样板实现，适合执行方；路径不相交让两边并行而不违背「后来者只读」的本意。这与 session-orchestration「指挥会话不亲自改代码」有出入，作为 T6 回灌该技能的实证：判断密集且量小的代码由指挥方做，前提是路径与执行方不相交。
- **D18：首启教程等「应用就绪」再启动；教程进行中任何确认框弹出即暂停** — 就绪 = 配置已加载 + 协议已同意 + 启动期两条异步链（`reconcileStartupInputMode` 的管理员恢复确认、`showProfileNotice` 的净化提示）都已结束，没弹的立即算结束，不用定时猜；`ConfirmProvider` 暴露「有确认框打开」的布尔并入 `paused`。不采用「任何 Overlay 打开就暂停」：设置弹窗是教程要打开的目标，那样 settings / profiles 两组会把自己藏起来。理由：用户截图实证首启气泡压在「以管理员模式恢复游戏模式」上（I12）。 自动更新：就绪门另含 `updateNotice === null && !applyingUpdate`（`update-ready` 弹窗先于教程到达时不启动），`update-available` 与 `update-downloading` 不阻塞；「重启并更新」中断教程不处理，introShown 未置位，新版首启会再进。
- **D19：步骤级前置只用现有 `prepare` 返回 `'skip'`，不做跨组流转** — 规则组开头加条件实操步「先加一条规则」（有按压规则则跳过），依赖规则卡的步骤在列表为空时跳过；引擎解析规则卡内锚点先在 `rule-latest` 内找（I13）。用户原话「复杂的流程流转可以先不做，保证教程设计得合理不会自己卡住就行」；跨组跳转需要返回栈与完成语义，收益不抵复杂度。落地 e89b77c。

## 3. 问题清单

| #   | 问题                                                                                                             | 严重度 | 位置                                                          | → 任务卡  |
| --- | ---------------------------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------- | --------- |
| I1  | 路线图 v0.3「首次引导」未实现，新用户只能靠 README                                                               | P1     | `docs/ROADMAP.md:809-811`                                     | T1–T3     |
| I2  | 同一使用事实在 README / CLAUDE.md / ROADMAP 当前阶段 / site content.ts 四处重复，无校验，已可见措辞漂移          | P1     | `README.md:23-30`、`CLAUDE.md` 输入模式段、`content.ts:69-94` | T4        |
| I3  | `.claude/skills/*` 三条软链指向不存在的 `D:/Workspace/burtst`，Claude Code 加载不到任何仓内技能                  | P1     | `.claude/skills/`                                             | T4（D11） |
| I4  | 无稳定锚点：五个标题栏按钮同类名，教程无法可靠选中目标                                                           | P1     | `PanelApp.tsx:1970-2030`                                      | T2        |
| I5  | `Overlay` 只在 `version` 变化时重算位置，不跟随目标滚动 / 窗口尺寸；规则列表可滚动，气泡会错位                   | P2     | `Overlay.tsx:321-361`                                         | T1        |
| I6  | `Overlay` 的 `zIndex: 99999` 硬编码，不读 `--fb-z-overlay-content` Token；引导层要叠在设置弹窗上，层级必须成体系 | P2     | `Overlay.tsx:316,332,343`、`theme.css:87-90`                  | T1        |
| I7  | `OverlayRoot` 的根级 Escape 只关最顶层 managed overlay；引导打开时按 Esc 会关掉设置弹窗而不是退出引导            | P2     | `Overlay.tsx:163-177`                                         | T1        |
| I8  | 无首启标记：协议同意后再无「新用户」信号，老用户升级也会被当新用户                                               | P2     | `PanelApp.tsx:796-804`                                        | T2（D8）  |
| I9  | 横版下 `switchLayout` 会拦 DD 模式，教程若硬切布局会触发拦截提示                                                 | P2     | `PanelApp.tsx:1407`                                           | T3（D9）  |
| I10 | CLAUDE.md 约 200 行，一半是架构正文，每次会话整份进入上下文；xwink-console 已用索引表形态                        | P2     | `CLAUDE.md`                                                   | T4        |
| I11 | pre-commit / CI 都不校验文档结构，教程与 reference 脱节不会被发现                                                | P2     | `.githooks/pre-commit`、`.github/workflows/ci.yml`            | T4        |
| I12 | 首启教程只等协议弹窗，不等启动期异步 `confirm()`（以管理员模式恢复输入模式、部分配置已调整），气泡压在确认框上；教程 `paused` 也不覆盖确认框 | P1     | `PanelApp.tsx` 首启 effect、`TourRunner` 挂载处 `paused`      | T2        |
| I13 | 规则卡内锚点每张卡都有一份，引擎取全局首个匹配，而实操判定看最后一条规则：老用户带多条规则跑教程高亮第一张、判定最后一张 | P1     | `TourRunner.tsx` `queryTarget`                                | T1 修补   |
| I14 | 没有按压规则时「规则玩法」第 2、3 步与「上手三步」跳过添加后的第 3、4 步指着空处讲                                     | P1     | `tours/rules.ts`、`tours/getting-started.ts`                  | T3 修补   |

覆盖维度核对：正确性（I5/I7/I9）、数据（I8：settings 键加法，无迁移风险）、安全（无：教程不接网络、不渲染外部内容）、性能（I5：跟随重算用 rAF + ResizeObserver，不轮询）、可观测（教程开始 / 完成 / 跳过写一条 `log_from_frontend` info，便于事后看新用户走到哪一步）、兼容（老用户升级路径见 D8）、回滚（第 7 节）。

## 4. 任务卡

### T1 — 引导引擎（TourRunner） · 进度: 已完成（fa6e4bd）

- **Fixes:** I5、I6、I7
- **Change:**
  - 新建 `apps/main/src/windows/panel/tour/types.ts`：`TourStep { id; target?: string; title; body; placement?: Location; prepare?(host): void | Promise<void> | 'skip'; done?(snapshot): boolean; actionHint?: string }`、`TourDef { id; title; summary; steps }`、`TourHost`、`TourSnapshot`。
  - 新建 `tour/TourRunner.tsx` + `TourRunner.css`：四块遮罩 + 高亮框 + 气泡（标题 / 正文 / 进度 `2 / 6` / 上一步 / 下一步 或 「完成」/ 跳过）。定位复用 `Overlay.tsx` 抽出的 `computePos` / `clampPos`（改为导出，不复制）。目标定位：`document.querySelector('[data-tour="<id>"]')`，展示前 `scrollIntoView({ block: 'nearest' })`，`ResizeObserver` + `scroll`（capture）+ `resize` 触发重算；目标缺失时最多等 1.5s（rAF 轮询）再回退为居中气泡。实操步骤（有 `done`）镂空区放开点击、每次宿主快照变化时判定并自动前进。Esc 退出引导（`keydown` capture 阶段 `stopPropagation`，不落到 `OverlayRoot`）。
  - `theme.css` 新增 `--fb-z-tour-mask: 100010` / `--fb-z-tour-bubble: 100020`；`Overlay.tsx` 三处硬编码改读 `var(--fb-z-overlay-content)`（顺手修 I6）。
  - `tour/useTourStore.ts`：读写 `settings.json` 的 `tours` 键（D7），暴露 `completed` / `markCompleted(id)` / `introShown` / `markIntroShown()`。
- **Acceptance:** 用一组临时静态步骤能从头走到尾；滚动规则列表时高亮框与气泡跟着走；亮暗主题与任意门派色下气泡可读；Esc / 跳过立即关闭且不影响下方弹窗；实操步骤在条件满足时自动进入下一步。
- **Test:** `pnpm lint`、`pnpm format:check`、`tsc --noEmit`；`pnpm dev` 真机走一遍（亮 / 暗各一次）。
- **Rollback:** 引导层是独立目录 + 一个挂载点，删挂载即恢复；settings 键多余无害。
- **Depends on:** 无

### T2 — 锚点、宿主接口、目录弹窗与首启触发 · 进度: 已完成（dcde753 含 D18 追加，d5）

- **Fixes:** I4、I8
- **Change:**
  - `PanelApp.tsx` / `HorizontalLayout.tsx` / `SettingsDialog.tsx` 加 `data-tour` 锚点：`layout-toggle` `menu` `minimize` `tabs` `rule-latest` `rule-key` `rule-interval` `rule-enable` `add-hold` `add-toggle` `add-group` `rule-advanced` `profile` `input-mode` `global` `settings-tabs` `settings-hotkeys` `settings-sound` `settings-general-theme` `settings-general-close` `hkb-keyboard` `hbar-interval` `hbar-legend`。
  - `PanelApp.tsx` 组装 `TourHost`（D10）并挂 `<TourRunner host={...} />`；☰ 菜单「关于」上方加「新手教程」；新建 `dialogs/TourCatalogDialog.tsx`（VS Code 风格：每组标题、简介、步骤数、已完成 ✓、按钮「开始」/「重新学习」）。
  - 首启触发（D8）：初始加载完成且协议不需同意（或 `handleAgreed` 后）→ 若 `!introShown && rules.length === 0` 自动开始 `getting-started`；否则 `markIntroShown()`。开始 / 完成 / 跳过各写一条 `log_from_frontend`。
- **Acceptance:** 新装（空配置）首启协议同意后自动进入上手教程；有规则的老用户升级后无打扰、菜单可见入口；目录弹窗正确显示完成状态并可重学。 启动期确认框（管理员恢复 / 净化提示）存在时首启教程不出现、点掉后才开始（D18）；教程中弹出确认框时气泡退让、关掉后回到同一步。
- **Test:** 清空 `app_data_dir` 模拟新装；保留配置模拟升级；两条路径各走一遍。
- **Rollback:** 同 T1；`data-tour` 属性对样式与行为无副作用。
- **Depends on:** T1

### T3 — 六组教程内容 · 进度: 已完成（e89b77c，flair-bloom-02 [099339]）

- **Fixes:** I1、I9
- **Change:** `tour/tours/{getting-started,game-mode,rules,layouts,profiles,settings}.ts` + `tour/tours/index.ts`（导出有序数组，id 即文件名，供 T4 校验）。
  - `getting-started`（实操 3 步）：标题栏与两个页签 → 点「+ 添加按压连发规则」（done: 规则数增加） → 在按键框按下一个键（done: 新规则 `target_key` 非默认） → 勾选启用 → 点亮「全局已禁用」（done: `globalEnabled`）→ 完成语（提示菜单里还有更多教程）。
  - `game-mode`：底部输入模式按钮 → 三种模式含义 → 游戏模式要装驱动、重启、管理员 → 诊断修复入口。
  - `rules`：按压 vs 切换页签 → 规则卡各字段 → 高级设置（启动键≠连发键）→ 互斥分组 → 多规则自动控速说明。
  - `layouts`：切布局按钮（`prepare` 按 `coincident_toggle` 决定 `'skip'` 整组或继续）→ 键鼠图点键三态 → 统一间隔与图例 → 最小化到浮窗。
  - `profiles`：底部配置按钮 → 新建 / 切换 → 设置 → 配置文件（导入导出 .qzh）→ 托盘切换。
  - `settings`：设置 → 热键（三个热键、修饰键、去重）→ 声音四个时机 → 通用（明暗 / 门派色 / 关闭行为 / 开机自启 / 自动更新）。
- **Acceptance:** 六组都能从目录进入并走完；实操步骤确实要做完才前进；横版组在 DD 模式下显示「当前输入模式不支持」而不是报错。
- **Test:** 真机逐组走一遍；`getting-started` 在竖版 / 横版两种初始布局下都能开始（横版下 `prepare` 先切竖版）。
- **Rollback:** 内容文件独立，可逐组下线。
- **Depends on:** T2

### T4 — 仓内 skill 与文档收口 · 进度: 已完成（指挥方 a34e3de；T4-a 基建由 d5 完成 a5595a7 / e89b77c）

- **Fixes:** I2、I3、I10、I11
- **Change:**
  - 新建 `skills/flair-bloom/SKILL.md`（frontmatter `name: flair-bloom`，「按任务读取」索引 + 事实边界）；`references/manual/<六个教程 id>.md`（每篇：这组教程解决什么、步骤大纲、常见坑，与 `tour/tours/<id>.ts` 一一对应）；`references/engineering/{architecture,key-policy,profile-format,updater,release,development}.md`（正文从 CLAUDE.md「关键架构决策」「输入约束」「发版流程」「协作规范」与目录结构迁入，逐条不丢）。
  - `CLAUDE.md` 改为索引：项目一句话 + 常用命令 + 「按任务读取」表（任务 → reference）+ 本仓特有硬约束一览（发版授权、schema 护栏）。
  - `README.md` 按 D12 收口；「玩法」段改成索引表指向 `references/manual/*`。
  - 按 D15 拆分 `docs/ROADMAP.md`：现状进 skill engineering references，未做项与设想进 README「接下来」，桌宠 / 许可证设计改写为 `docs/roadmaps/{pet-mode,license}.md`（`Status: draft`），删除原文件；CLAUDE.md 发版流程里「核对 docs/ROADMAP.md」改为核对 skill 与 README；`docs/roadmaps/` 建 `archive/`。
  - `scripts/check-skills.ts` + `package.json` `skills:check`：校验 `tour/tours/index.ts` 的 id 集合 == `references/manual/*.md` 文件集合；SKILL.md / CLAUDE.md / README.md 里的相对链接都存在。`.githooks/pre-commit` 在暂存 `.md` 或 `tour/` 文件时运行。
  - 按 D11 修复 `.claude/skills` 三条软链。
- **Acceptance:** `pnpm skills:check` 绿；CLAUDE.md 不超过 80 行；`docs/ROADMAP.md` 不存在且 `git grep ROADMAP.md` 无残留引用；ROADMAP 中每个 `[ ]` 项都能在 README「接下来」找到；`git grep` 任一架构关键词（如 `coincident_toggle`、`SIM_MARKER`）在 CLAUDE.md 与 skill 里只出现在一处正文。
- **Test:** `pnpm skills:check`、`pnpm format:check`；新开一个 Claude Code 会话验证按索引能定位到 reference。
- **Rollback:** 纯文档，`git revert`。
- **Depends on:** T3（manual 大纲要与教程一致）；engineering 部分不依赖任何任务卡，可先行。

### T5 — 收口与验证 · 进度: 已完成（d5；门禁全绿，六组真机走通，环境还原）

- **Change:** `CHANGELOG.md` `[Unreleased]` 新增「新手教程」条目；全量 `pnpm lint` / `format:check` / `tsc --noEmit` / `cargo check`（Rust 无改动，仅确认）；`pnpm dev` 按 T1–T3 的验收清单真机走一遍，亮暗各一次；提交。
- **Acceptance:** 门禁全绿；六组教程真机走通；CHANGELOG 与 ROADMAP 已更新。
- **Depends on:** T1–T4

### T6 — 回灌 wink-skills：重构为 project-planning · 进度: 已完成（wink-skills 两笔本地提交；pitfalls 待 T5 结论后可再补）

- **Fixes:** D16
- **Change（在 `D:/Workspace/wink-skills` 仓，独立提交）:**
  - `requirement-roadmap/` 改名 `project-planning/`，SKILL.md 重写为三层：① 项目文档单一来源——仓内 `skills/<project>/SKILL.md` + `references/`，入口文件（CLAUDE.md / AGENTS.md / README）只做索引，校验脚本钉住教程 / 文档 / 链接的一致性；② 计划列表——README「接下来」一节，每条一行，不分已定与设想；③ 变更路线图——每改动一份 `docs/roadmaps/<change>.md`，`Status: draft / active / completed / archived`，模板、任务卡、工期表与归档规则沿用现有正文。保留「快速评估 vs 完整路线图」两档与全部核心原则。
  - `legacy-onboarding/SKILL.md`：「规划迭代」一步改为「按 project-planning 建立文档源与计划列表」，去掉 ROADMAP + ARCHITECTURE 产出；description 同步。
  - `invoice/` 吞并为 `project-planning/references/invoice.md` + `templates/invoice.html`，只在用户说「开 invoice / 出账单 / 结算 / 请款」时按需读取；SKILL.md description 各给一句规划与开票的触发短语；wink-skills README 的 🔒 个人专用标记从技能级挪到该章节级（收款方与费率仍只写在该 reference 里）。
  - `README.md` 技能表与互链段落、`session-orchestration` 里对 `requirement-roadmap` 的引用全部改名；`ROADMAP.md` 覆盖地图更新。
  - `PROJECTS.md` 补 flair-bloom 条目（本轮协作内容与入口）。
  - 本机 `~/.claude/skills/requirement-roadmap` 软链改为 `project-planning`。
  - 本文件第 2 节引用的「requirement-roadmap 标准」改指 project-planning。
  - `session-orchestration`：把 D17 的分工模式写进 commander.md（判断密集的文档活由指挥方并行做、接口既定的实现交执行方；路径不相交时允许同时写、各自 pathspec），本轮新踩的坑进 pitfalls.md，PROJECTS.md 同条目记溯源。
- **Acceptance:** `grep -r requirement-roadmap` 在 wink-skills 与 `~/.claude/skills` 无残留；新开会话能以 `/project-planning` 触发；legacy-onboarding 不再产出 ROADMAP.md；说「开 invoice」能命中 project-planning 并只读 invoice 那一篇 reference。
- **Test:** 新会话里对本仓再跑一次 `/project-planning` 快速评估，确认读得到 `skills/flair-bloom` 与 README 列表。
- **Rollback:** `git revert` 单提交 + 软链改回。
- **Depends on:** T5（趁踩过的坑还热）

## 5. 依赖与顺序

```
T1 ─> T2 ─> T3 ─> T4(manual) ─> T5 ─> T6
             T4(engineering) ───┘
```

- **执行顺序：** T1 → T2 → T3 → T4 → T5 → T6（T6 在 wink-skills 仓）。T4 的 engineering 部分与 T1 无依赖，可穿插先做。
- **最小可交付里程碑：** T1 + T2 + T3 的 `getting-started` 一组 + T5。这已满足路线图「协议同意后引导创建第一条规则」；其余五组与文档收口可下一轮补。

## 6. 工期

口径：单人加 AI 协作，单位「会话」（1 会话约一个连续工作段），与 xwink-console 路线图同口径。校准来源：本仓无既有估 vs 实表，按 git 历史可比工作量——横版键鼠图 + 浮窗 + 互斥（三个 feat）在 2026-06-29 一天内落地，Markdown 渲染器 + 按键策略收敛在 2026-09-09 一天内落地，均为 1 会话量级；xwink-console `app-site.md` 里静态站骨架 0.5、内容页 1–2。

| 任务卡   | 预估 | 实际 | 偏差说明 |
| -------- | ---- | ---- | -------- |
| T1       | 1    | ✅   |          |
| T2       | 1    | ✅   | 追加 D18 就绪门（用户截图实证） |
| T3       | 1    | ✅   |          |
| T4       | 2    | ✅   |          |
| T5       | 0.5  | ✅   | 追加 D18 / D19 的走查；DD 模式下 layouts 禁用分支未真机验证（见范围外） |
| T6       | 1    | ✅   | 与 T5 并行做，未等 T5 收口 |
| **合计** | 6.5  | ✅   |          |

最大不确定项：T1 的「实操步骤放开镂空区点击 + 四块遮罩」在 WebView2 里的事件穿透表现，以及 `KeyCapture` 在遮罩之下能否正常聚焦录入；若不行改为「实操步骤不盖遮罩、只画高亮框」，不影响工期量级。不占净工时的项：无外部等待。

## 7. 风险与回滚

- **R1 引导与更新 / 协议弹窗抢层级：** 更新就绪弹窗可能在教程中途弹出。策略：`TourRunner` 监听 `showAgreement` / `updateNotice` 任一为真时暂停并隐藏，恢复后继续当前步骤；不做「排队」。
- **R2 目标元素找不到（用户已删除规则、切了布局）：** 等 1.5s 后回退居中气泡并显示「这一步的目标当前不在界面上」，可继续下一步；不抛错。
- **R3 老用户被强推教程：** D8 的空规则判定；若反馈仍打扰，加 `settings.json` 开关是一行改动。
- **R4 文档收口丢内容：** T4 验收要求 CLAUDE.md 现有段落逐条能在 skill 里找到；提交前 `git diff --stat` 核对删除行数 ≈ 迁入行数。
- **回滚：** 引导层与 skill 都是新增目录，`git revert` 单个提交即可；settings.json 新键无需清理。

## 8. 范围外 · follow-ups

- **DD 模式下 layouts 目录禁用分支只做了静态复核**：`available` 纯函数 + 目录按 `reason` 置 `aria-disabled`、onClick 直接 return。真机未验证，原因见下一条。
- **待查：dev 模式下「以管理员重启」起不来窗口**：提权实例报 `WebView2 error 0x800700AA 请求的资源在使用中`，只剩空壳窗口、输入后端回落 SendInput；提权进程也不继承 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`，CDP 断连。疑为 `tauri dev` 重拉实例 + 远程调试端口占用 WebView2 用户数据目录所致，`relaunch_as_admin` 的 `--await-pid` 机制本身无问题。需在生产包上复现一次再定性，不按缺陷记。
- **跨组流转**（前置不足跳到另一组再返回）：用户拍板不做（D19）。

- 内置规则模板（FPS / MOBA 一键导入）：`docs/ROADMAP.md:811` 第二项，本轮不做。
- 产品页 `apps/site/app/utils/content.ts` 的文案副本接到 skill（D14）。
- CI 增加前端门禁作业（`pnpm lint` / `format:check` / `tsc` / `skills:check`）：当前 `ci.yml` 只有 Rust，前端全靠 pre-commit；值得单独一卡。
- 浮窗（`panel-float.html`）内的引导：浮窗是另一个 WebView，教程只在面板窗口讲「怎么去浮窗」，不进浮窗。
- 桌宠相关教程：v0.4 桌宠落地后再加一组。
