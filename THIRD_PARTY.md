# 第三方组件声明

## Interception Driver

- 项目：[Interception](https://github.com/oblitum/Interception)
- 作者：Francisco Lopes (oblitum)
- 许可证：LGPL-3.0（非商业用途）
- 用途：内核级键盘输入模拟，用于兼容使用 Raw Input API 的游戏（如剑网三）
- 集成方式：
  - `interception-sys` crate（v0.1.3）— 编译期静态链接用户态库
  - `install-interception.exe`（v1.0.1）— 打包为应用资源，供用户安装内核驱动

### 说明

Interception 驱动为可选组件，对应输入模式「游戏模式」。默认的「通用模式」（SendInput）不依赖此驱动。

## DD 虚拟键鼠 SDK

- 项目：[ddxoft/master](https://github.com/ddxoft/master)
- 作者：ddxoft
- 用途：虚拟键鼠驱动，作为「游戏模式」之外的备用输入通道，兼容部分游戏模式仍无效的程序
- 集成方式（按原作者发布的二进制原样打包，未做修改或重新签名）：
  - `apps/main/src-tauri/resources/dd63330.dll` — 「DD驱动」模式使用的 DD DLL
  - `apps/main/src-tauri/resources/ddhid.63340.dll` — DDHID 用户态 DLL（当前模式入口已暂停）
  - `apps/main/src-tauri/resources/ddhid-driver/` — DD-HID 驱动包：
    - `ddc.exe` — PnP 安装/卸载工具
    - `ddhid63340.sys` / `ddhid63340.cat` / `ddhid63340.inf` — WHQL 签名的 HID-Class 驱动
- 运行时仅通过 `LoadLibraryW` + `GetProcAddress` 调用 DD SDK 导出函数：`DD_btn`、`DD_key`、`DD_todc`、`DD_whl`

### 说明

「DD驱动」模式使用 `dd63330.dll`，独立于 DDHID 驱动安装/卸载链路，要求宿主进程以管理员身份运行。

DDHID 模式（面板入口名称「DDHID」）为可选组件，但因稳定性风险当前已暂停开放；如系统中已安装 DDHID 驱动，建议在应用「诊断修复」中卸载。

DDHID 驱动无法标识自身注入的按键，因此使用DDHID 模式时「目标键」不能与「触发键 / 停止键」重合，本软件已在配置校验和模式切换处强制此约束。「DD驱动」模式不继承该同键限制。

如对 DD 驱动的来源、签名或行为有疑问，请以原作者仓库说明为准。
