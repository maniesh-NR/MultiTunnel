//! `multitunnel-tray` — Windows notification-area UI (implemented stage S3).
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    eprintln!("multitunnel-tray: the tray UI lands in stage S3");
    std::process::exit(2);
}
