# 数据格式：配置文件、设置、协议记录、许可证

## `.qzh` 配置文件

```
┌────────────────────────────────────────┐
│ Magic       4 bytes   "QZHU"           │
│ Version     1 byte    0x01             │
│ Flags       2 bytes   reserved         │
│ Nonce      12 bytes   随机（每次写入）  │
├────────────────────────────────────────┤
│ Ciphertext  N bytes   AES-256-GCM 密文  │
├────────────────────────────────────────┤
│ Auth Tag   16 bytes   GCM 认证标签      │
└────────────────────────────────────────┘
```

- `FileHeader`（19 字节）定义在 `packages/qzh-format/src/header.rs`；`magic + version + flags` 作为 AAD，文件头篡改即验证失败。
- 密钥派生 HKDF-SHA256，输入为内嵌 32 字节应用常量（`packages/crypto/src/aes.rs` 的 `MASTER_KEY`）。它只作混淆、不保密、不轮换：首个版本起所有用户的 `.qzh` 都用它加密，换掉会让老配置与跨版本分享全部解不开；仓库公开，密钥也能从二进制取出，换了没有收益。真要轮换时用文件头 `flags` 记密钥编号并回退旧钥解密。
- 高层入口：`qzh_format::read_encrypted(path)` / `write_encrypted(path, &T)` 封装 header + aad + decrypt + parse / serialize + encrypt + atomic-rename；`qzh_profile::load_from_path` / `save_to_path` 在此之上叠加 schema 迁移、`clamp_intervals` 与业务校验。
- 宏序列文件复用同一格式，schema 由 `macro_seq.rs` 定义（`MAX_STEPS = 256`）。

## JSON schema 与迁移（策略 B）

解密后的 JSON 首字段 `schema_version` 驱动 `qzh-profile/src/schema_migrate.rs` 的迁移链（调用 `packages/migrate` 的 `run_migrations()` 泛型运行器）。读取：`version == CURRENT` 直接反序列化；`< CURRENT` 按序执行迁移函数后以新 schema 写回；`> CURRENT` 拒绝加载（`TooNew`）并提示升级应用。每个版本一个迁移函数，不允许跳版本。

**新增字段原则**：一律 `#[serde(default)]` 走向后兼容；仅字段重命名 / 移动 / 删除 / 类型变更才递增 `schema_version`。高风险版本禁止裸 bump（原因见 [release](release.md) 护栏）。

当前 `CURRENT_SCHEMA_VERSION = 4`：

| schema_version | 变更                                                                        | 引入版本 |
| -------------- | --------------------------------------------------------------------------- | -------- |
| 1              | 初始 schema                                                                 | v0.1     |
| 2              | 按键字段裸 `u32` VK → `KeyId`（键盘 + 鼠标 5 键统一），可选字段 `null` 保留 | v0.2     |
| 3              | `MouseButton` 新增 `WheelUp` / `WheelDown`                                  | v0.2.4   |
| 4              | `BurstRule` 新增可选 `group` 字段（Toggle 互斥分组）                        | v0.2.5   |

`tauri-plugin-store` 的 `settings.json` 复用同一迁移基础设施。已知键：`closeBehavior`、`holdInGroupHintDismissed`（某组变成长按与切换混排时的插队提示已勾「不再提示」）、`sound`、`theme`、`layout`、`autoUpdate`（与后端 `bootstrap/update.rs` 同名同读）、`autoEnableOnStart`（与 `bootstrap/startup.rs` 同名同读）、`tours`（`{ completed: string[], introVersion: string }`，`introVersion` 是已自动展示过的教程版本，与协议版本同一套语义）、`runAsAdmin`（「以管理员模式启动」的用户意图，注册表标志被更新抹掉时靠它自愈）、当前激活配置路径。开机自启与「以管理员模式启动」不在这里：前者由 `tauri-plugin-autostart` 写 `HKCU\Run`，后者写 `HKCU\...\AppCompatFlags\Layers`，都以注册表为准，界面只读回显。

## 输入约束

| 参数                 | 范围                                  | 执行位置                                                               |
| -------------------- | ------------------------------------- | ---------------------------------------------------------------------- |
| 连发间隔（结构下限） | 1ms – 10000ms                         | `qzh-profile/src/profile.rs::validate()`（默认 10ms）                  |
| 连发间隔（有效下限） | ≥ 10ms（`MIN_EFFECTIVE_INTERVAL_MS`） | 加载时 `Profile::clamp_intervals()` 钳位 + `burst-engine` 运行时 floor |
| 单配置规则数         | ≤ 64                                  | `profile.rs::validate()`                                               |
| 宏序列步骤数         | ≤ 256                                 | `macro_seq.rs::MAX_STEPS`                                              |

约束在 `qzh-profile` 校验层执行，不依赖前端；按键合法性不在 `validate()` 里查，见 [key-policy](key-policy.md)。

## 配置文件管理

后端命令：`save_profile` / `load_profile` / `list_profiles` / `init_default_profile` / `get_active_profile_path` / `rename_profile` / `delete_profile` / `fork_active_profile`；外部导入 `commands/import_profile.rs`（支持丐帮高手 `config.json` 扫描、预览、导入）。默认配置受保护：修改默认配置自动 fork 成新配置。

## 用户协议记录

启动时先检查协议：store 里 `{ agreed, agreed_at, agreement_version, app_version_at_agree }`，未同意或 `agreement_version` 与代码硬编码的 `AGREEMENT_VERSION`（`bootstrap/agreement.rs`）不一致时面板展示协议页并屏蔽其他路由；正文滚动到底才激活同意；拒绝则退出。正文 `apps/main/src/assets/EULA.md` 以 `?raw` 内联进包。条款要点：使用风险自担、仅供个人娱乐与学习、禁止商用、免责、知识产权。当前协议 v1.4。

## 许可证（billing 离线签发）

- 格式：EdDSA（Ed25519）JWS 紧凑串，与 xwink-console 的 billing 约定。头部 `alg: EdDSA`、`typ: xwink-license+jwt`、`kid`；载荷 `iss: billing`、`aud: flair`、`jti`（许可证 id）、`ent`（权益键数组）、`iat`，永久许可证不带 `exp`。`crypto::verify_license` 依次校验 `alg`、`typ`、`kid`、签名、`iss`、`aud`、时间；`typ` 把 xwink 的权益凭证（`xwink-entitlement+jwt`）挡在外面。
- 时间：`iat` 允许领先本机 24 小时，超出判时钟回拨——Windows 上时区设错让系统 UTC 偏几小时很常见，回拨本来只是顺手的一道；`exp` 到点即过期。
- 公钥：私钥只在 billing。`crypto/build.rs` 把公钥按 `kid` 编进二进制，`kid` = SPKI DER 的 SHA-256 做 base64url 取前 22 字符（与 billing 同算法），构建时由 JWK 的 `x` 重算核对，不一致即失败。
  - `FLAIR_LICENSE_PUBKEYS`：billing 交付的 JWK 数组，`release.yml` 从同名 Secret 注入。发版构建（无 `debug_assertions`）缺失或为空串即失败；本地发版构建不需要激活时显式设为 `[]`。
  - `packages/crypto/license-dev-keys.json`：本地 billing 开发密钥的公钥，只在 `debug_assertions` 下编入。
  - 轮换：新版本加新钥，旧钥只验不签；止血时新版本移除旧钥，该钥签出的许可证全部失效、要重新激活。
- 权益：只有自定义水印署名 `flair.signature`（`crypto::ENT_SIGNATURE`）一项；其余功能对所有用户开放。激活与署名尚未接入应用，进度见 `docs/roadmaps/license.md`。
