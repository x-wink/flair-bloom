# 气质花 1.0.0 与官网发布

Status: archive（应用与官网已发布，安装包、签名、下载与更新端点验收通过）

## 范围与授权

用户要求「发版 1.0.0」，并明确选择应用和官网一起发布；追加要求发版前进行独立审查。本次授权覆盖应用 `v1.0.0` 的草稿构建、产物核验与公开，以及官网 `site-v1.0.0` 的部署与验收。

应用基线为 `v0.10.0`，官网基线为 `site-v0.1.11`。二者没有本次新增的运行时发布前置关系，在 main 门禁全绿后同批触发各自发布链。应用保留签名与 updater 配置、应用标识及配置 schema；不重新发布第一方服务端。

## 发布计划

本仓包含 Rust workspace、主应用 pnpm workspace 与独立安装依赖的 Nuxt 官网，不将官网并入根 workspace。按两个实际稳定 tag 分别核对 `git diff`：应用构建输入为 `apps/main/`、Rust workspace 和根版本／依赖文件；官网构建输入为 `apps/site/` 及其实际引用的应用主题、Markdown 解析器、协议与 Logo 样式。当前均有待发布变更，均需完整构建。仅支持 JavaScript workspace 制品的通用计划脚本不适用于本仓的桌面二进制与独立 Nuxt 项目，不为发版改动现有依赖边界。

GitHub Actions 的 CI、Coverage、Site 和 Release 均为 active。准备时发现的定长像素分块 lint 已修复，main 门禁通过后才触发发布；追加构建修复同样通过独立补审、本地检查与新的 main CI。Coverage 与官网的构建输入相较首个已验证候选没有变化，经 `git diff` 核对后沿用成功结果。

## 检查清单

- [x] 独立审查两个基线以来的完整差异，修复后补审。
- [x] 同步 1.0.0 版本、锁文件、最终公告与用户文档。
- [x] 本地前端／官网类型与构建、纯函数测试、Rust 格式／Clippy／测试、资源及文档检查通过。
- [x] 首个候选仅推 main，CI、Coverage、Site 全部成功。
- [x] 推 `v1.0.0` 与 `site-v1.0.0`，两条发布链成功。
- [x] 草稿安装包、签名、更新清单与版本匹配，官网明暗图片和公开下载通过。
- [x] 公开 1.0.0 并验证 GitHub Latest、第一方与旧版更新清单。
- [x] 记录发布结果与边界并归档。

## 验证边界与交接

首个候选源码为 `76c9834bcc7ade29d1a4bf45a3d923863b31d940`，main 门禁 [CI 43461800986](https://github.com/x-wink/flair-bloom/actions/runs/43461800986)、[Coverage 43461800777](https://github.com/x-wink/flair-bloom/actions/runs/43461800777)、[Site 43461800744](https://github.com/x-wink/flair-bloom/actions/runs/43461800744) 全绿后推两个 tag。

官网 [site-v1.0.0 发布链 43462245921](https://github.com/x-wink/flair-bloom/actions/runs/43462245921) 成功，真实线上页面的 14 种明暗／视口组合、脉冲、减少动态效果与上下留白验收通过。应用 [首轮发布链 43462246555](https://github.com/x-wink/flair-bloom/actions/runs/43462246555) 在打包前因 DLL 资源缺失失败，未产生公开版本。原因是 CI 新 Cargo 的目录布局与旧固定层级查找不兼容；追加修复识别两种布局，旧布局、新布局、缺失与复制保护共 9 项回归通过，独立补审无阻断项。

用户追加要求按上一稳定版本归并公告，1.0.0 公告已收敛为 3 项新增、8 项改进、1 项升级说明，删除新版内部二次修复与样式迭代流水，独立补审通过。为同步安装包内公告与构建修复，用户明确选择「允许替换，继续发布 1.0.0」，授权仅对未公开的应用 tag 破例。原注解 tag 对象 `9bae87992982bfa5cda789a7d53bc54f0d5fa558` 保留在本地 `backup/v1.0.0-first-76c9834`；通过精确租约替换应用 tag，官网 tag 保持原位。此授权不适用于后续版本。

独立审查发现并关闭一项 P2：启动加载已有配置未恢复窗口绑定，现统一经 `activate_profile_file` 激活，绑定状态及引擎均先于监听就绪。补审确认路径约束与失败回退完整，无遗留发布阻断项。Rust 新工具链的定长像素分块检查已修复，官网工作流补齐新共享 Logo 样式的路径过滤。

本地 Rust 全工作区格式、Clippy 和 472 项测试通过，7 项外部环境测试按原配置忽略；前端与官网的九项门禁通过，包含类型检查、生产构建、前端回归、资源与文档检查。官网浏览器已验证 14 种明暗／视口组合、原图链接、最小一屏及上下留白、脉冲与减少动态效果偏好。

检查不向真实游戏窗口发送按键，不修改用户当前配置，不执行正式安装器。驱动、DD 同键与游戏接收等既有真机边界沿用用户手册和对应调查记录，不将模拟测试等同于游戏验收。本轮日志放在系统临时目录 `flair-release-1.0.0`。用户要求保留的官网预览服务继续运行。

## 发布结果

- 应用源码为 `41013e586a41e0742a68db994439df6b66ec40aa`，[main CI 43464754441](https://github.com/x-wink/flair-bloom/actions/runs/43464754441) 与 [Release 43465254728](https://github.com/x-wink/flair-bloom/actions/runs/43465254728) 成功，现行 Rust stable 的真实 Windows 打包通过。官网源码仍为首个候选 `76c9834`，无需重新部署。
- [1.0.0](https://github.com/x-wink/flair-bloom/releases/tag/v1.0.0) 于北京时间 2026-11-14 00:50:07 公开并设为 Latest，包含 EXE、MSI、两份签名和 `latest.json`；旧稳定版本保留。
- 两个安装包的大小、SHA-256 与 GitHub 摘要一致，使用应用内 updater 公钥验证安装包签名和可信注释成功，篡改摘要被拒绝；三个 Windows 平台的更新地址与签名配对正确。
- EXE SHA-256 为 `71c94e86e5a99479e7e017d6e8acdfa3ae0741ba83c605808ff974530e142ec5`，MSI 为 `0c2f734ce20d5cab529473b773167c8bd96f15c93ac3f145c70c6b5fd63b1c2d`。
- 解包核验 NSIS、内含应用及 MSI 版本均为 1.0.0；NSIS 携带的图标与源码一致、驱动 DLL 与锁定依赖原件一致。MSI 保留升级码和内嵌快捷方式图标，包含 `interception.dll`。未执行安装器。
- 第一方 bootstrap、GitHub 直连与 gh-proxy 的更新清单均返回 1.0.0，与验收草稿一致；两个安装包公开下载均 HTTP 200。官网实际使用的公开版本源已返回 1.0.0，公告为最终合并文案。
- 临时验收浏览器均已退出；用户要求保留的本地官网服务继续监听 `http://127.0.0.1:3900/flair-bloom/`。
