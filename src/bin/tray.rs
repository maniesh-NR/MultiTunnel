//! `multitunnel-tray` — Windows notification-area UI (implemented stage S3).
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::io::Write;

fn main() {
    // GUI-subsystem note (S2-REV-B P2-5): without a console the stderr
    // handle can be invalid, and eprintln! PANICS on a failed write
    // (exit 101 instead of the intended 2). Write best-effort and ignore
    // the error.
    let _ = writeln!(
        std::io::stderr(),
        "multitunnel-tray: the tray UI lands in stage S3"
    );
    std::process::exit(2);
}
