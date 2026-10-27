//! Syno Mount: Synology shared folders on Linux over SSHFS.
//!
//! Without arguments the app opens its window. systemd starts the same program
//! with `run-mount <id>` (one mount) and `agent` (remounts after suspend and
//! network changes); those run without a window.

mod devices;
mod failure;
mod mounts;
mod setup;
mod state;

use std::process::ExitCode;

use syno_mount::{Paths, manager};
use tauri::Manager;

fn headless(arguments: &[String]) -> Option<ExitCode> {
    let paths = Paths::from_env();
    match arguments {
        [command, id] if command == "run-mount" => match manager::run_mount(&paths, id) {
            Ok(()) => Some(ExitCode::SUCCESS),
            Err(error) => {
                eprintln!("{error}");
                Some(ExitCode::FAILURE)
            }
        },
        [command] if command == "agent" => manager::agent(&paths),
        [flag] if flag == "--version" => {
            println!("syno-mount {}", env!("CARGO_PKG_VERSION"));
            Some(ExitCode::SUCCESS)
        }
        _ => None,
    }
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = headless(&arguments) {
        return code;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second start brings the running window to the front instead.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            app.manage(state::AppState::new(Paths::from_env()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            setup::discover,
            setup::known_devices,
            setup::forget_device,
            setup::connect,
            setup::connect_ssh_only,
            setup::setup_services,
            setup::enable_services,
            setup::scan_host_key,
            setup::install_key,
            setup::more_shares,
            setup::create_mounts,
            setup::cancel_setup,
            mounts::system_check,
            mounts::list_mounts,
            mounts::start_mount,
            mounts::stop_mount,
            mounts::update_mount,
            mounts::remove_mount,
            mounts::self_test,
            mounts::speed_test,
            mounts::open_folder,
            mounts::command_line,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Syno Mount");
    ExitCode::SUCCESS
}
