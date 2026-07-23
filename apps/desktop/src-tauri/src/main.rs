// ElectronIx MES desktop shell (Tauri 2). The UI is the React app in ../dist;
// this binary just hosts the webview. Kiosk and supervisor console are the same
// bundle — the landing screen is chosen by role after login (§11).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running ElectronIx MES desktop");
}
