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
- 用途：HID-Class 虚拟键鼠驱动，用于兼容更严格的 Raw Input 类游戏（如剑网三、部分 FPS 游戏）
- 集成方式（按原作者发布的二进制原样打包，未做修改或重新签名）：
  - `apps/main/src-tauri/resources/ddhid.63340.dll` — DD-HID 用户态 DLL
  - `apps/main/src-tauri/resources/ddhid-driver/` — DD-HID 驱动包：
    - `ddc.exe` — PnP 安装/卸载工具
    - `ddhid63340.sys` / `ddhid63340.cat` / `ddhid63340.inf` — WHQL 签名的 HID-Class 驱动
- 运行时仅通过 `LoadLibraryW` + `GetProcAddress` 调用三个导出函数：`DD_btn`、`DD_key`、`DD_todc`

### 说明

DD-HID 模式（面板入口名称「究极HID」）为可选组件，要求宿主进程以管理员身份运行。

DD 驱动无法标识自身注入的按键，因此使用究极HID 模式时「目标键」不能与「触发键 / 停止键」重合，本软件已在配置校验和模式切换处强制此约束。

如对 DD 驱动的来源、签名或行为有疑问，请以原作者仓库说明为准。
