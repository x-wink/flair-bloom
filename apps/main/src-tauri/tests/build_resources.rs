#[path = "../build_support.rs"]
mod build_support;

use std::{
    fs::{self, File, FileTimes},
    time::{Duration, SystemTime},
};

use build_support::{copy_if_changed, find_interception_dll};

#[test]
fn legacy_layout_finds_dll_and_executable_directory() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("target/x86_64-pc-windows-msvc/release");
    let out = profile.join("build/flair-bloom-123/out");
    let source = profile.join("build/interception-sys-456/out/interception.dll");
    fs::create_dir_all(&out).unwrap();
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"dll").unwrap();

    assert_eq!(find_interception_dll(&out).unwrap(), (source, profile));
}

#[test]
fn cargo_v2_layout_finds_dll_and_executable_directory() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("target/x86_64-pc-windows-msvc/release");
    let out = profile.join("build/flair-bloom/123/out");
    let source = profile.join("build/interception-sys/456/out/interception.dll");
    fs::create_dir_all(&out).unwrap();
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"dll").unwrap();

    assert_eq!(find_interception_dll(&out).unwrap(), (source, profile));
}

#[test]
fn missing_dll_reports_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("debug/build/flair-bloom/123/out");
    fs::create_dir_all(&out).unwrap();

    assert_eq!(
        find_interception_dll(&out).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn invalid_layout_reports_invalid_input() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(
        find_interception_dll(dir.path()).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
}

#[test]
fn creates_missing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.dll");
    let destination = dir.path().join("destination.dll");
    fs::write(&source, b"dll").unwrap();

    copy_if_changed(&source, &destination).unwrap();

    assert_eq!(fs::read(destination).unwrap(), b"dll");
}

#[test]
fn repeated_sync_preserves_destination_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.dll");
    let destination = dir.path().join("destination.dll");
    fs::write(&source, b"dll").unwrap();
    copy_if_changed(&source, &destination).unwrap();
    // 固定旧时间，避免文件系统时间精度让无条件复制也碰巧通过。
    File::options()
        .write(true)
        .open(&destination)
        .unwrap()
        .set_times(
            FileTimes::new().set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)),
        )
        .unwrap();
    let modified = fs::metadata(&destination).unwrap().modified().unwrap();

    copy_if_changed(&source, &destination).unwrap();
    copy_if_changed(&source, &destination).unwrap();

    assert_eq!(
        fs::metadata(destination).unwrap().modified().unwrap(),
        modified
    );
}

#[test]
fn updates_destination_when_same_length_contents_differ() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.dll");
    let destination = dir.path().join("destination.dll");
    fs::write(&source, b"new").unwrap();
    fs::write(&destination, b"old").unwrap();

    copy_if_changed(&source, &destination).unwrap();

    assert_eq!(fs::read(destination).unwrap(), b"new");
}

#[test]
fn missing_source_preserves_destination() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("destination.dll");
    fs::write(&destination, b"old").unwrap();

    let error = copy_if_changed(&dir.path().join("missing.dll"), &destination).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(fs::read(destination).unwrap(), b"old");
}

#[test]
fn reports_copy_failure() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.dll");
    fs::write(&source, b"dll").unwrap();

    assert!(copy_if_changed(&source, &dir.path().join("missing/destination.dll")).is_err());
}
