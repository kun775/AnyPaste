mod clipboard;
mod commands;
mod events;
mod hotkey;
#[cfg(windows)]
mod input;
mod janitor;
mod paths;
#[cfg(windows)]
mod permanent;
mod state;
mod store;
mod tray;
mod windows;

use tauri::{Manager, RunEvent};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

use crate::{paths::AppPaths, state::AppState, store::Store};

pub fn run() {
    let paths = AppPaths::init().expect("无法创建数据目录");

    let app = tauri::Builder::default()
        // 必须最先注册：重复启动时由已运行的实例打开设置窗口，新进程直接退出
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_settings(app);
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .target(Target::new(TargetKind::Stdout))
                .target(Target::new(TargetKind::Folder {
                    path: paths.log_dir.clone(),
                    file_name: Some("anypaste".into()),
                }))
                .level(log::LevelFilter::Info)
                .max_file_size(1024 * 1024)
                .rotation_strategy(RotationStrategy::KeepOne)
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .build(),
        )
        .plugin(hotkey::plugin())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .manage(paths)
        .invoke_handler(tauri::generate_handler![
            commands::hide_popup,
            commands::open_settings,
            commands::is_paused,
            commands::list_clips,
            commands::clip_counts,
            commands::paste_clip,
            commands::copy_clip,
            commands::set_permanent,
            commands::delete_clip,
            commands::clear_temp,
            commands::app_info,
        ])
        .setup(|app| {
            log::info!("AnyPaste {} 正在启动", app.package_info().version);
            let paths = app.state::<AppPaths>();
            let store = Store::open(&paths.db_file)?;
            // 先于剪贴板监听执行，避免误删监听线程刚写入的图片
            match janitor::sweep_orphan_images(&store, &paths) {
                Ok(0) => {}
                Ok(n) => log::info!("清理孤立图片 {n} 个"),
                Err(e) => log::warn!("清理孤立图片失败: {e:#}"),
            }
            app.manage(store);

            tray::create(app.handle())?;
            windows::setup_popup(app.handle())?;
            janitor::spawn(app.handle().clone());
            #[cfg(windows)]
            clipboard::spawn(app.handle().clone());
            hotkey::register_defaults(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("初始化 Tauri 应用失败");

    app.run(|_app, event| {
        // 关闭最后一个窗口时不退出，保持托盘常驻；只有 app.exit() 带退出码才真正退出
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
