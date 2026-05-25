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

Interception 驱动为可选组件。用户在"设置 → 输入模式"中手动切换为"驱动模式"时才会使用。
默认的"标准模式"（SendInput）不依赖此驱动。
