# 发版应急与回退 Runbook

发布后发现版本有严重问题（卡键、无法启动、配置损坏等）时的应急操作手册。
命令以仓库 `x-wink/flair-bloom` 为例，按需替换版本号。

## 核心前提：Tauri updater 只升不降

`tauri-plugin-updater` 仅在 **远端版本 > 本机版本** 时才更新，**永远不会降级**。
因此「回退」对两类用户含义不同：

| 用户群 | 能做什么 | 走哪条路径 |
| --- | --- | --- |
| 还没升到坏版本 | 拦住，别让他们升上去 | 路径 A（止血） |
| 已经升到坏版本 | 无法降级，只能用更高版本盖掉 | 路径 B（向前滚） |
| 已静默下载暂存 `pending_update` | 召不回，下次启动必装 | 只能等路径 B 的新版覆盖 |

> 结论：本项目的「紧急回退」实质 = **紧急向前滚一个修复版**，不是把版本号往回拨。

---

## 路径 A：止血（拦住未升级用户，分钟级）

更新端点 `releases/latest/download/latest.json` 跟着 GitHub 的「Latest」指针走。
把 Latest 改回上个好版本，`latest.json` 就不再分发坏版本。

```sh
# 1. 确认当前 Latest 指向谁
gh api repos/x-wink/flair-bloom/releases/latest --jq '.tag_name'

# 2. 把上个好版本（如 v0.2.7）重新标记为 Latest
gh release edit v0.2.7 --repo x-wink/flair-bloom --latest

# 3.（可选，更彻底）把坏版本转回 Draft，使其资产/页面下线
gh release edit v0.2.8 --repo x-wink/flair-bloom --draft=true

# 4. 复核 Latest 已切回
gh api repos/x-wink/flair-bloom/releases/latest --jq '.tag_name'
```

止血只对 **尚未升级** 的客户端有效；已升级或已暂存 `pending_update` 的用户不受影响，必须靠路径 B 救。

---

## 路径 B：向前滚修复（救已升级用户，一个 CI 周期）

发一个 **更高版本号** 的修复版，让自动更新把所有人（含已中招用户）带走。

```sh
# 1. 回退坏改动（按需 revert 一个或多个提交）
git revert <坏提交SHA>            # 或手动改回，提交为 fix(...)

# 2. 在 CHANGELOG.md 的 [Unreleased] 写明本次修复

# 3. bump 一个更高 patch 号（脚本同步版本号 + 重命名 changelog 节）
pnpm bump-version 0.2.9
git add -A && git commit -m "chore(release): bump version to 0.2.9"

# 4. 打 tag 并推送，触发 CI
git tag v0.2.9
git push origin main && git push origin v0.2.9

# 5. 监控构建（约 8–10 分钟）
gh run watch <run-id> --exit-status --interval 30

# 6. 构建成功后核对 Draft 产物与 latest.json 版本
gh release view v0.2.9 --repo x-wink/flair-bloom --json isDraft,assets \
  --jq '{isDraft,assets:[.assets[].name]}'

# 7. 发布并标记 Latest
gh release edit v0.2.9 --repo x-wink/flair-bloom --draft=false --latest
gh api repos/x-wink/flair-bloom/releases/latest --jq '.tag_name'   # 应为 v0.2.9
```

---

## 发版护栏（让回退始终安全）

1. **高风险版本禁止裸 bump `CURRENT_SCHEMA_VERSION`**。
   一旦坏版本写出更高 schema 的 `.qzh`，回退到旧 schema 的代码会 `TooNew` 拒载、**砸用户配置**——使回退变成单向门。
   新字段一律用 `#[serde(default)]` 走兼容路径，不动 schema 版本号。
2. **风险改动尽量挂运行时开关**，出事先关开关，不必全量回退。
3. **上个稳定版的 Release 和 `.sig` 签名产物不要删**，确保向前滚 / 手动回退有可用且已签名的产物。

---

## 速查：发布后验证命令

```sh
# 构建结论
gh run view <run-id> --json status,conclusion --jq '{status,conclusion}'

# Release 是否已发布、产物清单
gh release view vX.Y.Z --repo x-wink/flair-bloom --json isDraft,isPrerelease,assets \
  --jq '{isDraft,isPrerelease,assets:[.assets[].name]}'

# Latest 指针解析（updater 实际会读到的版本）
gh api repos/x-wink/flair-bloom/releases/latest --jq '.tag_name'

# latest.json 内容（版本与下载 URL）
tmp=$(mktemp -d); gh release download vX.Y.Z --repo x-wink/flair-bloom --pattern latest.json --dir "$tmp"
cat "$tmp/latest.json"
```

> 降险基建（min-version / kill-switch、灰度发布、延迟自动安装）见 `docs/ROADMAP.md` v0.6，可缩小坏版本「即时全量铺开」的爆炸半径。
