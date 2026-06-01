mod audio_engine;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            audio_engine::get_devices,
            audio_engine::start_routing,
            audio_engine::stop_routing,
            audio_engine::update_receiver,
            audio_engine::get_receiver_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
