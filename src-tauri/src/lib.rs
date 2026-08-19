mod downloader;
mod updater;

use tauri::{Manager, Window};

#[tauri::command]
async fn ensure_libs(app: tauri::AppHandle) -> Result<(), String> {
    downloader::ensure_libs(&app).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn ytdlp_version(app: tauri::AppHandle) -> Result<String, String> {
    downloader::ytdlp_version(&app).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_ytdlp(app: tauri::AppHandle) -> Result<String, String> {
    downloader::force_update_ytdlp(&app).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn probe(
    app: tauri::AppHandle,
    url: String,
    cookies_from_browser: Option<String>,
) -> Result<downloader::Probe, String> {
    let cookies = cookies_from_browser.as_deref().filter(|s| !s.is_empty());
    downloader::probe(&app, &url, cookies)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn download(
    app: tauri::AppHandle,
    window: Window,
    url: String,
    format: String,
    mp3_bitrate: Option<u16>,
    dest_dir: String,
    indices: Vec<u32>,
    cookies_from_browser: Option<String>,
) -> Result<(), String> {
    let fmt = downloader::AudioFormat::parse(&format).map_err(|e| e.to_string())?;
    let cookies = cookies_from_browser.as_deref().filter(|s| !s.is_empty());
    downloader::download(&app, &window, &url, fmt, mp3_bitrate, &dest_dir, &indices, cookies)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // fuerza la creación del directorio de datos temprano
            let _ = app.path().app_data_dir();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ensure_libs,
            probe,
            download,
            ytdlp_version,
            update_ytdlp
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar tauri");
}
