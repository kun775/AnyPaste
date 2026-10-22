use std::{fs, io, thread, time::Duration};

use tauri::{AppHandle, Manager};

use crate::{
    events,
    paths::AppPaths,
    settings::{CaptureSettings, SettingsState},
    store::{now_ms, Store},
};

/// 过期检查间隔。查询走索引，开销很小，间隔短可让过期时间更精确。
const INTERVAL: Duration = Duration::from_secs(60);

/// 定期删除过期和超出数量上限的临时条目。
pub fn spawn(app: AppHandle) {
    let spawned = thread::Builder::new().name("janitor".into()).spawn(move || {
        let store = app.state::<Store>();
        let paths = app.state::<AppPaths>();
        loop {
            let settings = app.state::<SettingsState>().get().capture();
            match cleanup(&store, &paths, &settings, now_ms()) {
                Ok(0) => {}
                Ok(_) => events::clips_changed(&app),
                Err(e) => log::warn!("清理临时条目失败: {e:#}"),
            }
            thread::sleep(INTERVAL);
        }
    });
    if let Err(e) = spawned {
        log::error!("无法创建清理线程: {e}");
    }
}

/// 删除过期的临时条目，再按数量上限裁剪，并删除对应图片文件。返回删除的条目数。
pub fn cleanup(store: &Store, paths: &AppPaths, settings: &CaptureSettings, now: i64) -> anyhow::Result<usize> {
    let expired = store.purge_expired(now)?;
    let trimmed = if settings.max_temp_items > 0 {
        store.trim_temp(settings.max_temp_items)?
    } else {
        Default::default()
    };
    remove_images(paths, &expired.image_files);
    remove_images(paths, &trimmed.image_files);
    if expired.rows > 0 || trimmed.rows > 0 {
        log::info!("已删除临时条目：过期 {} 条，超出上限 {} 条", expired.rows, trimmed.rows);
    }
    Ok(expired.rows + trimmed.rows)
}

/// 删除 images 目录中没有被任何条目引用的文件，例如写入后入库失败留下的图片。
/// 必须在剪贴板监听启动前调用，否则可能删掉刚写入、尚未入库的图片。
pub fn sweep_orphan_images(store: &Store, paths: &AppPaths) -> anyhow::Result<usize> {
    let referenced = store.image_files()?;
    let mut removed = 0;
    for entry in fs::read_dir(&paths.images_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_file() && !referenced.contains(&name) {
            remove_image(paths, &name);
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn remove_images(paths: &AppPaths, files: &[String]) {
    for file in files {
        remove_image(paths, file);
    }
}

fn remove_image(paths: &AppPaths, file: &str) {
    match fs::remove_file(paths.images_dir.join(file)) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("删除图片 {file} 失败: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::clips_tests::text_clip;

    fn setup() -> (tempfile::TempDir, AppPaths, Store) {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::under(dir.path().to_path_buf());
        fs::create_dir_all(&paths.images_dir).unwrap();
        (dir, paths, Store::open_in_memory().unwrap())
    }

    fn with_image(store: &Store, paths: &AppPaths, name: &str, at: i64) {
        fs::write(paths.images_dir.join(format!("{name}.png")), b"png").unwrap();
        let mut clip = text_clip(name);
        clip.image_file = Some(format!("{name}.png"));
        store.upsert_clip(&clip, at, 1000).unwrap();
    }

    #[test]
    fn cleanup_removes_expired_and_over_limit_with_files() {
        let (_dir, paths, store) = setup();
        with_image(&store, &paths, "expired", 0);
        with_image(&store, &paths, "old", 900);
        with_image(&store, &paths, "new", 950);
        let settings = CaptureSettings { max_temp_items: 1, ..Default::default() };

        assert_eq!(cleanup(&store, &paths, &settings, 1000).unwrap(), 2);
        assert!(!paths.images_dir.join("expired.png").exists());
        assert!(!paths.images_dir.join("old.png").exists());
        assert!(paths.images_dir.join("new.png").exists());
    }

    #[test]
    fn sweep_removes_only_unreferenced_files() {
        let (_dir, paths, store) = setup();
        with_image(&store, &paths, "kept", 0);
        fs::write(paths.images_dir.join("orphan.png"), b"png").unwrap();

        assert_eq!(sweep_orphan_images(&store, &paths).unwrap(), 1);
        assert!(paths.images_dir.join("kept.png").exists());
        assert!(!paths.images_dir.join("orphan.png").exists());
    }
}
