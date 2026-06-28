mod ambient_commands;
mod commands;
mod diamond_overlay;
mod setup_commands;
mod task_cards;

use ambient_commands::*;
use commands::*;
use setup_commands::*;

pub fn run() {
    let app_state = create_app_state().expect("Failed to create app state");

    let ambient_state = AmbientState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .manage(ambient_state)
        .invoke_handler(tauri::generate_handler![
            send_message,
            get_conversation,
            list_models,
            switch_model,
            scan_devices,
            control_device,
            list_personas,
            switch_persona,
            get_sync_status,
            search_knowledge,
            export_data,
            delete_data,
            get_ambient_status,
            check_setup_status,
            install_ollama,
            pull_model,
            complete_setup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
