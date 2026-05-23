# 气质花 — 资源表

开发过程中持续维护，每项资源含使用场景、占位路径、尺寸格式、详细描述（可直接用作 AI 生图提示词）。

---

## 应用图标

### 主图标

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src-tauri/icons/icon.png` |
| 格式 | PNG，多尺寸（32 / 128 / 256 / 512px），另需 `.ico`（Windows）|
| 使用场景 | 任务栏、开始菜单、安装包、关于页面 |
| 内容描述 | 一朵风格化的花卉图形，整体圆形构图，花瓣 5-6 片，线条简洁现代。主色调粉紫渐变（#C084FC → #818CF8），背景透明。风格介于扁平与轻拟物之间，适合小尺寸下仍清晰可辨。 |
| AI 提示词 | `app icon, stylized flower, 5 petals, flat design with subtle depth, pink to purple gradient #C084FC to #818CF8, clean lines, transparent background, recognizable at 32px, modern UI icon style, no text` |

### 托盘图标 — 启用状态

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src-tauri/icons/tray-active.png` |
| 格式 | PNG，16×16px 和 32×32px（高 DPI） |
| 使用场景 | 连发功能启用时的系统托盘图标 |
| 内容描述 | 主图标的简化版本，花卉轮廓清晰，色彩饱和，表示"活跃"状态。花朵中心有细微发光效果或高亮点。 |
| AI 提示词 | `system tray icon 16x16, stylized flower silhouette, vibrant pink-purple color, glowing center, active/enabled state, pixel-perfect at 16px, transparent background` |

### 托盘图标 — 禁用状态

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src-tauri/icons/tray-inactive.png` |
| 格式 | PNG，16×16px 和 32×32px |
| 使用场景 | 连发功能全局关闭时的系统托盘图标 |
| 内容描述 | 与启用状态相同图形，但整体灰度化（#9CA3AF），无发光效果，视觉上明显区别于启用态。 |
| AI 提示词 | `system tray icon 16x16, stylized flower silhouette, grayscale #9CA3AF, dimmed/disabled state, no glow, transparent background, same shape as active version` |

---

## 桌宠角色

### 基础角色立绘（参考图）

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/pet/assets/character-reference.png` |
| 格式 | PNG，512×512px，透明背景 |
| 使用场景 | 角色设计参考，用于指导各状态动画的绘制 |
| 内容描述 | 一只小型拟人化猫娘或精灵角色，头身比约 1:1.5，圆润可爱风格。头部特征：大眼睛、花朵发饰（呼应应用主题）、小猫耳或精灵耳。整体配色以粉紫为主（与应用图标呼应）。表情温和友好，站立姿势轻松。线条干净，适合做成精灵图动画。 |
| AI 提示词 | `cute chibi cat girl character, 1:1.5 head-body ratio, big sparkly eyes, flower hair accessory, small cat ears, pink and purple color scheme, clean line art, transparent background, anime style, full body standing pose, suitable for sprite animation, kawaii` |

### 动画帧 — Idle（待机）

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/pet/assets/anim-idle.svg` |
| 格式 | SVG（CSS 关键帧动画）或 PNG Sprite Sheet（4帧，每帧 160×160px，横排） |
| 使用场景 | 无操作时的默认状态，循环播放 |
| 内容描述 | 角色静立，胸口有轻微呼吸起伏（上下 2-3px），每 3-4 秒随机眨眼一次（眼睛闭合 2 帧）。尾巴（如有）缓慢左右摆动。整体节奏悠闲，不引人注意。 |
| 帧序列 | 帧1：标准站立；帧2：轻微上移（呼吸吸气）；帧3：眨眼；帧4：标准站立 |

### 动画帧 — Burst（连发激活）

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/pet/assets/anim-burst.svg` |
| 格式 | SVG 或 PNG Sprite Sheet（6帧，每帧 160×160px） |
| 使用场景 | 连发引擎激活期间持续循环 |
| 内容描述 | 角色进入兴奋/专注状态。眼睛变成专注的倒三角或星星眼，双手/爪子快速交替按键动作，身体有轻微左右抖动（±3px），花朵发饰随之颤动。节奏较快（每帧约 80ms）。整体传达"高速运作"的感觉。 |
| AI 提示词 | `chibi character excited/focused expression, star eyes or concentrated eyes, hands typing rapidly, slight body vibration, hair accessory shaking, energetic pose, same character design as idle` |

### 动画帧 — Hover（鼠标悬停）

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/pet/assets/anim-hover.svg` |
| 格式 | SVG 或 PNG Sprite Sheet（4帧，每帧 160×160px） |
| 使用场景 | 鼠标进入桌宠窗口区域时 |
| 内容描述 | 角色抬头看向鼠标方向，眼睛追踪（通过 CSS transform 实现瞳孔偏移），尾巴竖起或摇摆加快，表情变得好奇。有轻微"被发现了"的惊喜感。 |

### 动画帧 — Alert（配置切换）

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/pet/assets/anim-alert.svg` |
| 格式 | SVG 或 PNG（2帧，每帧 160×160px） |
| 使用场景 | 切换配置文件时短暂播放（约 0.8 秒），播完回到前一状态 |
| 内容描述 | 角色耳朵突然竖起，眼睛睁大，举起一只爪子做"注意"动作，头顶出现感叹号或小星星。快速播放后恢复。 |

### 动画帧 — Sleep（睡眠）

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/pet/assets/anim-sleep.svg` |
| 格式 | SVG 或 PNG Sprite Sheet（4帧，每帧 160×160px） |
| 使用场景 | 空闲超过 N 分钟（用户可配置）后进入 |
| 内容描述 | 角色闭眼，身体微微弯曲或坐下，头顶漂浮 ZZZ 气泡（透明度渐变循环），呼吸幅度比 Idle 更大更慢。整体温柔安静。 |

---

## UI 界面资源

### 空状态插图（无规则时）

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/panel/assets/empty-rules.svg` |
| 格式 | SVG，推荐尺寸 200×160px |
| 使用场景 | 规则列表为空时展示，引导用户添加第一条规则 |
| 内容描述 | 简约线条风格插图，一朵花和一个空白的键盘按键，或桌宠角色探头看着空列表的场景。配色与主题一致（粉紫），线条轻盈不沉重。 |
| AI 提示词 | `empty state illustration, minimalist line art, small cute character peeking at empty list, flower motif, pink purple palette, friendly and inviting, SVG style, no text` |

### 错误状态插图（文件损坏等）

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/panel/assets/error-state.svg` |
| 格式 | SVG，推荐尺寸 200×160px |
| 使用场景 | 配置文件损坏、引擎启动失败等错误状态展示 |
| 内容描述 | 桌宠角色做出困惑或惊讶表情，旁边有一个破裂的花朵或叹号图标。整体不过于严肃，保持可爱风格。 |

### 激活成功插图

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/windows/panel/assets/license-success.svg` |
| 格式 | SVG，推荐尺寸 200×160px |
| 使用场景 | 兑换码激活成功后的庆祝画面 |
| 内容描述 | 桌宠角色举手庆祝，周围有花瓣飘落和星星效果，表情喜悦。传达"解锁成功"的愉悦感。 |

---

## 落地页资源（apps/release-server/static/）

### 截图 — 面板模式

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/release-server/static/assets/screenshot-panel.png` |
| 格式 | PNG，1280×800px，2x 高清（实际展示 640×400） |
| 使用场景 | 落地页功能介绍区截图展示，体现配置界面 |
| 内容描述 | 应用面板模式截图，展示规则列表界面。窗口圆角，有轻微阴影，背景为浅色系。规则列表清晰可见，显示 2-3 条示例连发规则，界面整洁现代。 |
| 备注 | 应用开发完成后截取真实界面，占位期间可用 UI mockup 替代 |

### 截图 — 桌宠模式

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/release-server/static/assets/screenshot-pet.png` |
| 格式 | PNG，800×600px |
| 使用场景 | 落地页桌宠模式介绍，展示浮在游戏画面上的桌宠 |
| 内容描述 | 游戏画面（模糊处理保护版权）的右下角浮有桌宠角色，角色处于 Burst 激活状态（兴奋动画帧），体现"游戏中使用"的场景感。 |
| 备注 | 应用完成后实拍，注意避免展示具体游戏版权内容 |

### Hero 背景图

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/release-server/static/assets/hero-bg.svg` |
| 格式 | SVG，全宽自适应 |
| 使用场景 | 落地页顶部 Hero 区域背景装饰 |
| 内容描述 | 抽象花瓣和光点散布的背景图案，粉紫渐变色系（#C084FC → #818CF8 → #1E1B4B 深色底）。花瓣轮廓半透明，营造轻盈梦幻感。图案密度低，不遮挡前景文字。 |
| AI 提示词 | `abstract SVG background, scattered flower petals and light particles, pink to purple gradient, semi-transparent, dreamy and light, suitable as hero section background, low density pattern, dark base #1E1B4B` |

### 功能图标组

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/release-server/static/assets/icons/feature-*.svg` |
| 格式 | SVG，每个 48×48px |
| 使用场景 | 落地页"功能亮点"区域，每个功能配一个图标 |
| 需要的图标 | `feature-burst.svg`（连发）/ `feature-macro.svg`（宏）/ `feature-pet.svg`（桌宠）/ `feature-safe.svg`（离线安全）/ `feature-profile.svg`（配置文件） |
| 内容描述 | 线条风格图标，统一粗细（2px stroke），配色使用紫色系（#8B5CF6），简洁表意清晰，适合小尺寸展示。 |

### OG 社交分享图

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/release-server/static/assets/og-image.png` |
| 格式 | PNG，1200×630px |
| 使用场景 | 分享链接时社交平台展示的预览图（og:image） |
| 内容描述 | 左侧展示 App 名称"气质花"和副标题"游戏按键助手"（白色文字），右侧展示应用图标和桌宠角色立绘，整体背景为粉紫深色渐变。风格现代简洁，文字清晰可读。 |

---

## 音效（可选，低优先级）

### 连发启动音

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/assets/sounds/burst-start.ogg` |
| 格式 | OGG，时长 < 0.3 秒，音量轻柔 |
| 使用场景 | 连发模式激活时播放（可在设置中关闭） |
| 内容描述 | 轻快的短促提示音，类似小铃铛或像素风 "叮" 声，不刺耳，游戏中不突兀。 |

### 连发停止音

| 字段 | 内容 |
|------|------|
| 占位路径 | `apps/main/src/assets/sounds/burst-stop.ogg` |
| 格式 | OGG，时长 < 0.3 秒 |
| 使用场景 | 连发模式停止时播放 |
| 内容描述 | 比启动音略低沉的短促音，与启动音形成一对，听感上有"关闭"感。 |

---

## 资源状态追踪

| 资源 | 状态 | 备注 |
|------|------|------|
| 主图标 | ⬜ 待生成 | |
| 托盘图标（启用） | ⬜ 待生成 | |
| 托盘图标（禁用） | ⬜ 待生成 | |
| 角色参考立绘 | ⬜ 待生成 | 优先完成，其他动画依赖此设计 |
| 动画 — Idle | ⬜ 待生成 | |
| 动画 — Burst | ⬜ 待生成 | |
| 动画 — Hover | ⬜ 待生成 | |
| 动画 — Alert | ⬜ 待生成 | |
| 动画 — Sleep | ⬜ 待生成 | |
| 空状态插图 | ⬜ 待生成 | |
| 错误状态插图 | ⬜ 待生成 | |
| 激活成功插图 | ⬜ 待生成 | |
| 截图 — 面板模式 | ⬜ 待截图 | 应用完成后实拍 |
| 截图 — 桌宠模式 | ⬜ 待截图 | 应用完成后实拍 |
| Hero 背景图 | ⬜ 待生成 | |
| 功能图标组（5个） | ⬜ 待生成 | |
| OG 社交分享图 | ⬜ 待生成 | |
| 音效（连发启动） | ⬜ 待生成 | 低优先级 |
| 音效（连发停止） | ⬜ 待生成 | 低优先级 |
