// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if quantix_lib::run_update_rollback_helper_from_args() {
        return;
    }
    quantix_lib::run()
}
