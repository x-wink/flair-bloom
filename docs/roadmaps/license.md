# 许可证激活与亲友专属功能 — 设计稿

Status: draft
Goal: 用离线兑换码激活限时的亲友专属功能，到期自动降级不锁死；开工时转 active 并补任务卡与工期。

---

## 1. 背景

校验基础已就位：`packages/crypto/src/license.rs` 的 Ed25519 `verify_license` + `LicensePayload`，兑换码格式、payload 字段与密钥策略见 skill 的 [profile-format](../../skills/flair-bloom/references/engineering/profile-format.md)。未实现的是签发 CLI 的完整流程、激活 UI、按 `feature_bits` 控制功能开关，以及亲友专属功能本身。公钥当前为全零占位、AES 主密钥为编译期占位，发布前都要替换为构建注入。

## 2. 决策

- **D1：离线校验，不联网** — 理由：与「本地离线、除更新外不联网」的产品承诺一致；防时钟回拨靠 payload 的 `issue_time` 下界校验，`last_verified_at` 列为扩展优化点。
- **D2：私钥只在 `apps/keygen` 与 GitHub Secrets** — 不进主应用二进制。
- **D3：到期后自动降级，不崩溃、不锁死** — 亲友功能关闭，核心功能照常。
- **D4：鼠标连发保持对所有用户开放** — `MOUSE_BURST` 位预留，是否收敛为亲友专属在开工时决定。

## 3. 设计

### 许可证

- `apps/keygen` CLI：生成 Ed25519 密钥对，签名输出兑换码。
- 激活面板：输入兑换码、显示到期时间与已激活功能；许可证状态面板：剩余天数、激活时间、已授权功能列表。
- 引擎启动时读取激活记录，按 feature bits 控制功能开关。
- 到期前 7 天 UI 提醒（面板 banner + 桌宠状态气泡）。

### 亲友专属功能

| 功能                                                                                                       | 落点（拟）                                 |
| ---------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| 宏录制（事件流 + 时间戳，存为 `.qzh`，`MacroSequence` / `MAX_STEPS = 256`）与回放（原速 / 倍速）+ 热键绑定 | `apps/main/src-tauri/engine/macro_play.rs` |
| 随机抖动（间隔 ± 可配置随机偏差）                                                                          | `packages/burst-engine`                    |
| 条件配置集（检测前台进程自动切换配置）                                                                     | `apps/main/src-tauri/watcher.rs`           |
| 回放速度调节 UI（0.5x / 1x / 2x）                                                                          | 面板                                       |
| 桌宠扩展动画包                                                                                             | `apps/main/src/windows/pet/`（依赖桌宠）   |

## 4. 已知风险

- AES 密钥可被逆向提取：接受「防普通用户」定位，不对抗专业逆向。
- 系统时间回拨：`issue_time` 下界校验。

## 5. 待办（开工时展开成任务卡）

- [ ] keygen CLI 与 Secrets 注入
- [ ] 构建注入真实公钥与 AES 主密钥
- [ ] 激活面板与状态面板
- [ ] feature bits 功能开关与到期降级
- [ ] 宏录制回放、随机抖动、条件配置集
