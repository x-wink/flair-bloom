# 自动更新、更新公告与发布基础设施

## 自动更新

- 开关存 `settings.json` 的 `autoUpdate`（缺省视为开启），后端 `bootstrap/update.rs` 与前端 `PanelApp` 读同一个键。
- `check_and_download(app, trigger)` 是唯一的检查入口。`CheckTrigger::Startup` 才受开关约束、且「已是最新」不出声；`Manual`（菜单「检查更新」或标题栏提示）一律下载——用户已经表达了更新意图，开关只管「自动」那一档。
- 不下载时发 `update-available` 让标题栏亮提示；下载完成发 `update-ready` 弹窗（含 Release 正文），可「稍后处理」或「重启并更新」，安装由 `apply_pending_update` 命令触发；下载包暂存 `{app_local_data_dir}/pending_update/`。
- `update-downloading` 带 `silent` 标志，启动期自动下载只画进度条不弹 toast。`UpdateLock` 防止重入。
- 更新器只升不降：坏版本无法「降级召回」，应急路径见 [release](release.md) 的「应急回退」。

## 更新公告的两个来源不能混

菜单 / 关于里的「更新公告」（`UpdateNoticeDialog` 的 `mode='current'`）取 `panel/changelog.ts` 从随包内联的 `CHANGELOG.md` 切出的**当前运行版本**那一节，回答「我现在这版做了什么」；`mode='ready'` 才是刚下载完、还没装上的新版本的 `update.body`（来自 updater 接口）。两者若共用一个状态，用户在菜单里会看到一份自己还没装上的公告。

## Markdown 渲染

用户协议（`assets/EULA.md`）与更新公告（`update.body`）都由 `components/Markdown.tsx` 渲染，解析在同目录的 `markdown-parse.ts`（纯函数）。不引第三方库有两个硬理由：更新公告正文来自网络，任何走 `dangerouslySetInnerHTML` 的方案都会开出 XSS 面，这里只产出 React 元素、文本一律经 children 转义；现成渲染器会输出真的 `<a href>`，Tauri WebView 点一下就把面板导航走且无法返回，故链接只渲染文本、完整地址放 `title`（本应用未装 opener / shell 插件）。支持范围按两份文档实际用法划定：ATX 标题、`---` 分隔线、有序 / 无序列表（可嵌套）、段落、加粗、斜体、行内代码、链接；表格、引用块、围栏代码块、图片与内联嵌套不支持，超范围语法当普通文本显示。段落软换行按两侧是否为 CJK 决定要不要补空格。产品页 `apps/site` 直接引用这份解析器。

## 发布基础设施

| 职责         | 平台                               | 说明                                                                                                                                               |
| ------------ | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| 安装包托管   | GitHub Releases                    | `.exe` / `.msi` / `.nsis.zip` + `.sig`                                                                                                             |
| updater 端点 | GitHub Releases                    | `tauri-action` 每次发布上传 `latest.json`（Tauri 标准 manifest）                                                                                   |
| 私钥         | GitHub Actions Secrets             | `TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)`；Ed25519 许可证私钥同样只在签发 / 构建时注入                                                                |
| CI 构建      | GitHub Actions `release.yml`       | 推 `v*` tag 触发 Windows x64 构建、发布 Draft                                                                                                      |
| Release 正文 | `CHANGELOG.md`                     | `scripts/extract-changelog.ts` 提取当前版本节                                                                                                      |
| 产品页       | app.xwink.fun/flair-bloom          | `apps/site` 纯静态 SSG；域名与 server 块归 xwink-console 仓的 app-site 单元                                                                        |
| 发布镜像     | app.xwink.fun/flair-bloom/releases | 服务器每 10 分钟跟随 Latest 指针同步安装包，sha256 校验、整轮原子切换、改写 `latest.json` 的下载地址为镜像（签名只覆盖安装包内容，改写不影响校验） |

`tauri.conf.json` 的 updater endpoints 当前只指向 GitHub `releases/latest/download/latest.json`；镜像端点作为第一端点、GitHub 兜底的切换待随一次应用发版生效（见 README「接下来」）。产品页路径：`/flair-bloom/`（介绍、下载、公告、支持）、`/releases/releases.json`（公告与安装包清单）、`/releases/latest.json`、`/releases/<tag>/<安装包>`；`/flair-bloom/download` 打开即下载 Latest 的 exe（`?type=msi` 下 MSI）。产品页发版走 `site-v*` tag 与 `.github/workflows/site.yml`，不经 Tauri updater，详见 `apps/site/README.md`。
