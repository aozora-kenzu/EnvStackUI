#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    envstack_ui_lib::handle_prelaunch_wait();
    envstack_ui_lib::run()
}
