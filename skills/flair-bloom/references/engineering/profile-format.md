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
- 密钥派生 HKDF-SHA256，输入为内嵌 32 字节应用常量。**AES 主密钥当前为编译期常量占位符**（`packages/crypto/src/aes.rs` 顶部 `MASTER_KEY`），发布前需替换为 build script 注入的真实密钥。
- 高层入口：`qzh_format::read_encrypted(path)` / `write_encrypted(path, &T)` 封装 header + aad + decrypt + parse / serialize + encrypt + atomic-rename；`qzh_profile::load_from_path` / `save_to_path` 在此之上叠加 schema 迁移、`clamp_intervals` 与业务校验。
- 宏序列文件复用同一格式，schema 由 `macro_seq.rs` 定义（`MAX_STEPS = 256`，亲友功能）。

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

`tauri-plugin-store` 的 `settings.json` 复用同一迁移基础设施。已知键：`closeBehavior`、`holdInGroupHintDismissed`（长按与切换第一次混进同一组时的插队提示已勾「不再提示」）、`sound`、`theme`、`layout`、`autoUpdate`（与后端 `bootstrap/update.rs` 同名同读）、`autoEnableOnStart`（与 `bootstrap/startup.rs` 同名同读）、`tours`（`{ completed: string[], introVersion: string }`，`introVersion` 是已自动展示过的教程版本，与协议版本同一套语义）、`runAsAdmin`（「以管理员模式启动」的用户意图，注册表标志被更新抹掉时靠它自愈）、当前激活配置路径。开机自启与「以管理员模式启动」不在这里：前者由 `tauri-plugin-autostart` 写 `HKCU\Run`，后者写 `HKCU\...\AppCompatFlags\Layers`，都以注册表为准，界面只读回显。

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

## 许可证（Ed25519 离线校验）

- payload：`version u8` / `issue_time u64`（防时钟回拨下界校验）/ `expiry u64` / `features u32`（位掩码，见 `license.rs::feature_bits`）。
- 兑换码 `QZHUA-XXXXX-XXXXX-XXXXX-XXXXX`（Base32：64 字节签名 + JSON payload）。
- 私钥仅在 `apps/keygen` 使用、寄存 GitHub Secrets，不进主应用二进制；主应用只内置校验公钥，**当前为全零占位，发布前替换**。
- 功能分层：核心功能（按压 / Toggle 连发、键鼠滚轮、配置管理、自动更新）免费；亲友专属（宏录制回放、随机抖动、条件配置集、桌宠扩展动画包）由 `feature_bits` 控制。`MOUSE_BURST` 位预留但当前不限制——鼠标连发对所有用户开放。激活 UI 与功能开关尚未实现，设计见 `docs/roadmaps/license.md`（draft）。
