// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::panic;
use std::fs::File;
use std::io::Write;

fn main() {
    panic::set_hook(Box::new(|panic_info| {
        let msg = match panic_info.payload().downcast_ref::<&str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Box<Any>",
            },
        };
        let location = panic_info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown location".to_string());
        let log_msg = format!("Panic occurred: {} at {}\n", msg, location);
        
        if let Ok(mut file) = File::create("crash_log.txt") {
            let _ = file.write_all(log_msg.as_bytes());
        }
        eprintln!("{}", log_msg);
    }));

    tauri_app_lib::run()
}
