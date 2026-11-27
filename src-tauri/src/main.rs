#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    envstack_ui_lib::handle_prelaunch_wait();
    if let Some(code) = envstack_ui_lib::try_handle_cli() {
        std::process::exit(code);
    }
    envstack_ui_lib::run()
}
