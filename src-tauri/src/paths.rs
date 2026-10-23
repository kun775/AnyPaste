use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// 程序目录下存在此文件时进入便携模式。
const PORTABLE_MARKER: &str = "portable";

/// 应用数据目录约定：默认位于 `%APPDATA%\AnyPaste`；便携模式下位于程序目录的 `data` 子目录。
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
    pub images_dir: PathBuf,
    pub log_dir: PathBuf,
    /// 便携模式（不支持自动安装更新）。
    pub portable: bool,
}

impl AppPaths {
    /// 解析并创建所有目录，在 Tauri 启动前调用（日志插件需要日志目录）。
    pub fn init() -> io::Result<Self> {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        let portable_dir = exe_dir.as_deref().and_then(portable_data_dir);
        let portable = portable_dir.is_some();
        let data_dir = match portable_dir {
            Some(dir) => dir,
            None => dirs::data_dir()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法定位系统数据目录"))?
                .join("AnyPaste"),
        };
        let paths = Self { portable, ..Self::under(data_dir) };
        fs::create_dir_all(&paths.images_dir)?;
        fs::create_dir_all(&paths.log_dir)?;
        Ok(paths)
    }

    pub(crate) fn under(data_dir: PathBuf) -> Self {
        Self {
            db_file: data_dir.join("anypaste.db"),
            images_dir: data_dir.join("images"),
            log_dir: data_dir.join("logs"),
            data_dir,
            portable: false,
        }
    }
}

fn portable_data_dir(exe_dir: &Path) -> Option<PathBuf> {
    exe_dir.join(PORTABLE_MARKER).is_file().then(|| exe_dir.join("data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_under_data_dir() {
        let p = AppPaths::under(PathBuf::from("root"));
        assert_eq!(p.data_dir, PathBuf::from("root"));
        assert_eq!(p.db_file, PathBuf::from("root").join("anypaste.db"));
        assert_eq!(p.images_dir, PathBuf::from("root").join("images"));
        assert_eq!(p.log_dir, PathBuf::from("root").join("logs"));
    }

    #[test]
    fn portable_marker_switches_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(portable_data_dir(dir.path()), None);
        fs::write(dir.path().join(PORTABLE_MARKER), "").unwrap();
        assert_eq!(portable_data_dir(dir.path()), Some(dir.path().join("data")));
    }
}
