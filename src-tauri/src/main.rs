#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Redirect WebView2 user data into executable-adjacent portable directory
    // so the portable .exe leaves zero files in %APPDATA% or Windows Registry.
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let webview_dir = exe_dir.join("LumenHID.data").join("webview2_cache");
            let _ = std::fs::create_dir_all(&webview_dir);
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", webview_dir);
        }
    }

    lumen_hid_lib::run();
}
