use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn find_interception_dll(out_dir: &Path) -> io::Result<(PathBuf, PathBuf)> {
    // Cargo 的中间目录不是稳定契约，不能靠固定祖先层级推导 EXE 目录。
    let build_dir = out_dir
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "build"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "OUT_DIR 缺少 build 目录"))?;
    let profile_dir = build_dir
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "缺少构建 profile 目录"))?;

    for entry in fs::read_dir(build_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("interception-sys-") {
            let dll = entry.path().join("out/interception.dll");
            if dll.is_file() {
                return Ok((dll, profile_dir.to_path_buf()));
            }
        } else if name == "interception-sys" {
            for unit in fs::read_dir(entry.path())? {
                let dll = unit?.path().join("out/interception.dll");
                if dll.is_file() {
                    return Ok((dll, profile_dir.to_path_buf()));
                }
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("未在 {} 找到 interception.dll", build_dir.display()),
    ))
}

pub fn copy_if_changed(source: &Path, destination: &Path) -> io::Result<()> {
    let contents = fs::read(source)?;
    // 无变化时不能触碰目标：resources 下的写入会触发 Tauri 重启，EXE 同级 DLL 也可能正在使用。
    if fs::read(destination).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    fs::copy(source, destination)?;
    Ok(())
}
