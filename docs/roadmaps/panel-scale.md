# 面板等比缩放与主题对比度 — 改造路线图 / Change Roadmap

Status: active（代码完成并推送，待 Windows 真机验收后随下一版发布）
Goal: 大屏用户能把面板（及浮窗）等比放大；所有主题色在明暗两档下文字都过 WCAG AA 4.5:1。

---

## 1. 背景 / Background

亲友反馈：大屏上面板太小；粉色主题下灰色提示字看不清。实测灰字 #888 叠在主色渐变底上只有 2.3–2.8:1，20 个预设全部不达标；白字实底在绯粉、流金等 9 个亮色上也不过线。

## 2. 决策 / Decisions

- D1 对比度按 xwink-ui 规范：文字四档在三种 surface 与叠满淡染的面板底上都过 4.5:1，严格单调；面板主色淡染压到 `APP_TINT_MAX = 0.15`（作者选「背景减淡 + 文字换档」）。
- D2 实底前景按对比度取白或近黑，主色不改（作者选「主色不变，亮主色改用深字」）。
- D3 颜色算法在仓内 `contrast.ts` 移植 `@xwink/ui/brand`，不直接依赖：发版工作流无私有源凭据，Node 测试不剥离 node_modules 的 TS。界面迁到 Vue（vue-migration）后换回包里的实现。产品页已直接用包里的 `brandVariables`。
- D4 窗口拖边缩放，设计尺寸即最小尺寸，比例在拖动中由系统锁住：Windows 拦 `WM_SIZING`，macOS 设 NSWindow `contentAspectRatio`。不用「静止 400 ms 再吸附」——作者试用后嫌手感怪。
- D5 设计尺寸取常用比例：竖版 405×720（9:16），横版 1072×603（16:9，排版至少需要 1060×580，只往大取）。
- D6 竖版、横版各记一个比例，上限各按自己的设计尺寸卡当前屏幕工作区；浮窗不拖边，跟随面板当前布局的比例。

## 3. 任务卡 / Task cards

### T1 — 主题对比度 · 进度: 已完成（2026-10-29）

`theme.css` / `theme.ts` / `contrast.ts`，主色文字 `--fb-color-primary-text`、语义色文字 `--fb-color-*-text`，`scripts/theme-contrast.test.ts` 钉住 20 预设 × 明暗。浏览器 mock 截图核对过绯粉亮色、流金暗色、设置页。

### T2 — 面板与浮窗等比缩放 · 进度: 代码完成，待 Windows 真机验收

`windowScale.ts` / `useWindowScale.ts` / `commands/window.rs`（`lock_aspect` 单测）/ 浮窗跟随；设置 → 通用 → 外观显示缩放并可复位。Windows 侧 subclass 只在临时 crate 里对 Windows 目标做过类型检查与 clippy，完整交叉编译在 mac 上跑不了（ring 缺 MSVC 头文件）。

### T3 — Windows 真机验收 · 进度: 未开始

1. 无边框窗口拖四边与四角，比例锁住、不抖，被拖的边动、对边不动。
2. 拖到最小尺寸与屏幕工作区边缘时卡得准；150% 等高 DPI 下同样成立，跨不同 DPI 的屏幕拖动后比例不变。
3. 放大后横版走线、引导气泡位置不偏；切到浮窗后浮窗跟着放大；横版 1072×603 下键鼠图排版正常。

## 4. 交接 / Handoff

- **进度（2026-10-29）：** T1、T2 完成并推送，CHANGELOG `[Unreleased]` 已写「面板可放大」「文字更清楚」。
- **遗留：** T3 全部未做；mac 上的锁比例只在作者本机 `tauri dev` 里看过效果，mac 版仍不在发布范围。
- **下一步入口 = T3：** Windows 上 `pnpm dev`（真机冒烟起法见 [development](../../skills/flair-bloom/references/engineering/development.md)），按 T3 三条逐项过；问题修完再随下一版发布。
