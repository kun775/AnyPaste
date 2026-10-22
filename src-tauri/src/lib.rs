mod commands;
mod paths;
mod state;
mod tray;
mod windows;

use tauri::RunEvent;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

use crate::{paths::AppPaths, state::AppState};

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
        .manage(AppState::default())
        .manage(paths)
        .invoke_handler(tauri::generate_handler![
            commands::hide_popup,
            commands::open_settings,
            commands::is_paused,
            commands::app_info,
        ])
        .setup(|app| {
            tray::create(app.handle())?;
            windows::setup_popup(app.handle())?;
            log::info!("AnyPaste {} 已启动", app.package_info().version);
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
