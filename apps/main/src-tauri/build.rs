fn main() {
    tauri_build::build();

    #[cfg(windows)]
    copy_interception_dll();
}

/// `interception-sys` 把 `interception.dll` 复制到自己的 OUT_DIR 后供链接，但运行时
/// 仅 `cargo run` 会自动把 OUT_DIR 注入 dll 搜索路径。直接执行 `target/<profile>/flair-bloom.exe`
/// （例如 ShellExecuteEx 提权重启或安装后启动）会因找不到 interception.dll 进程加载失败。
/// 这里在编译时把 dll 复制到 `target/<profile>/`，与 EXE 同级，所有启动方式都可解析。
#[cfg(windows)]
fn copy_interception_dll() {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    // OUT_DIR = target/<profile>/build/<this-crate>-<hash>/out
    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let out = PathBuf::from(&out_dir);
    let Some(target_profile_dir) = out.ancestors().nth(3).map(|p| p.to_path_buf()) else {
        return;
    };
    let build_dir = target_profile_dir.join("build");

    let Ok(entries) = fs::read_dir(&build_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if !s.starts_with("interception-sys-") {
            continue;
        }
        let dll = entry.path().join("out").join("interception.dll");
        if !dll.exists() {
            continue;
        }
        let dest = target_profile_dir.join("interception.dll");
        if let Err(e) = fs::copy(&dll, &dest) {
            println!(
                "cargo:warning=复制 interception.dll 到 {} 失败: {}",
                dest.display(),
                e
            );
        }
        println!("cargo:rerun-if-changed={}", dll.display());
        return;
    }
}
