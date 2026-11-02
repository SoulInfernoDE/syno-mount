//! Syno Mount: Synology shared folders over SFTP, on Linux and Windows.
//!
//! Without arguments the app opens its window. `agent` keeps the folders mounted
//! (started by systemd on Linux, at sign-in on Windows), and on Linux systemd starts
//! `run-mount <id>` for each folder; those run without a window.

// No console window next to the app (or the agent) on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod admin;
mod devices;
mod failure;
mod mounts;
mod setup;
mod state;

use std::process::ExitCode;

use syno_mount::{MountStore, Paths, manager};
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
        // For scripts and support: the state of every folder, and a folder's self-test.
        [command] if command == "status" => {
            let store = MountStore::load(&paths.mounts());
            let statuses: Vec<_> = store
                .all()
                .iter()
                .map(|mount| manager::status(&paths, mount))
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&statuses).unwrap_or_default()
            );
            Some(ExitCode::SUCCESS)
        }
        [command, id] if command == "selftest" => {
            let store = MountStore::load(&paths.mounts());
            let Some(mount) = store.get(id) else {
                eprintln!("no folder with the ID {id}");
                return Some(ExitCode::FAILURE);
            };
            match syno_mount::selftest::run(&mount.local_path) {
                Ok(checks) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&checks).unwrap_or_default()
                    );
                    Some(ExitCode::SUCCESS)
                }
                Err(error) => {
                    eprintln!("{error}");
                    Some(ExitCode::FAILURE)
                }
            }
        }
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
            admin::admin_connect,
            admin::admin_overview,
            admin::admin_share_access,
            admin::admin_apply,
            admin::admin_enable_services,
            admin::admin_disconnect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Syno Mount");
    ExitCode::SUCCESS
}
