#[cfg(windows)]
mod build_support;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 必须先把 interception.dll 同步到 resources/，再交给 tauri_build：
    // tauri_build::build() 会校验 bundle.resources 里所有路径存在，否则中止。
    #[cfg(windows)]
    {
        copy_interception_dll()?;
        compile_diagnostics()?;
    }

    tauri_build::build();
    Ok(())
}

/// `interception-sys` 把 `interception.dll` 复制到自己的 OUT_DIR 后供链接，但运行时
/// 仅 `cargo run` 会自动把 OUT_DIR 注入 dll 搜索路径。直接执行 `target/<profile>/flair-bloom.exe`
/// （例如 ShellExecuteEx 提权重启或安装后启动）会因找不到 interception.dll 进程加载失败。
///
/// 复制目标有两处：
/// 1. `target/<profile>/interception.dll`：与 dev/release 的 EXE 同级，覆盖 `cargo run`、
///    `pnpm dev`、直接运行 release exe 等所有本地启动方式。
/// 2. `apps/main/src-tauri/resources/interception.dll`：让 `bundle.resources` 能把 DLL 打入
///    NSIS/MSI 安装包；NSIS hook 在 PostInstall 时再把它从 `$INSTDIR\resources\` 移动到
///    EXE 同级，保证安装后的运行环境也能解析。
#[cfg(windows)]
fn copy_interception_dll() -> Result<(), Box<dyn std::error::Error>> {
    use std::env;
    use std::path::PathBuf;

    let out = PathBuf::from(env::var("OUT_DIR")?);
    let (dll, target_profile_dir) = build_support::find_interception_dll(&out)?;
    let resources_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("resources");
    for destination in [
        target_profile_dir.join("interception.dll"),
        resources_dir.join("interception.dll"),
    ] {
        build_support::copy_if_changed(&dll, &destination)?;
    }
    println!("cargo:rerun-if-changed={}", dll.display());
    Ok(())
}

#[cfg(windows)]
fn compile_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, path::PathBuf, process::Command};
    let compiler = PathBuf::from(env::var_os("SystemRoot").ok_or("未找到 Windows 目录")?)
        .join(r"Microsoft.NET\Framework64\v4.0.30319\csc.exe");
    let output = PathBuf::from(env::var("OUT_DIR")?).join("input-diagnostics.exe");
    let mut command = Command::new(compiler);
    command.args([
        "/nologo",
        "/target:winexe",
        "/platform:x64",
        "/codepage:65001",
        "/optimize+",
        "/r:System.Windows.Forms.dll",
        "/r:System.Drawing.dll",
        "/r:System.Xml.Linq.dll",
    ]);
    command.arg(format!("/out:{}", output.display()));
    for name in ["DdDeliveryCheck.cs", "IncidentWindowsErrors.cs"] {
        let path = PathBuf::from("../../../scripts/diagnostics").join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        command.arg(path);
    }
    if !command.status()?.success() {
        return Err("无法编译 Windows 诊断程序".into());
    }
    let resources = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("resources");
    build_support::copy_if_changed(&output, &resources.join("input-diagnostics.exe"))?;
    let script = PathBuf::from("../../../scripts/diagnose-environment.ps1");
    println!("cargo:rerun-if-changed={}", script.display());
    build_support::copy_if_changed(&script, &resources.join("diagnose-environment.ps1"))?;
    Ok(())
}
