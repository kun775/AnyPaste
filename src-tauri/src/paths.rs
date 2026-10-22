use std::{fs, io, path::PathBuf};

/// 应用数据目录约定，Windows 上位于 `%APPDATA%\AnyPaste`。
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
    pub images_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    /// 解析并创建所有目录，在 Tauri 启动前调用（日志插件需要日志目录）。
    pub fn init() -> io::Result<Self> {
        let base = dirs::data_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法定位系统数据目录"))?;
        let paths = Self::under(base.join("AnyPaste"));
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
        }
    }
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
}
