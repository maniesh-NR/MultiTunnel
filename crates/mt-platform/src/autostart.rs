//! Launch-on-login (autostart) integration.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert D"): Windows uses the HKCU
//! `Software\Microsoft\Windows\CurrentVersion\Run` key via `winreg`
//! (cfg-gated; compiled but not exercised in this sandbox); Linux uses the
//! XDG autostart file `$XDG_CONFIG_HOME/autostart/multitunnel.desktop`
//! (falling back to `$HOME/.config/autostart`, same resolution as
//! [`crate::dirs::config_dir`]'s base — NOT inside the app's
//! `multitunnel` subfolder, per the XDG autostart spec).
//!
//! The "what value would we write" logic is pure and tested on every OS:
//! [`run_key_value`] (the quoted command for the Run key) and
//! [`desktop_entry`] (the desktop-file body).
//!
//! IMPLEMENTATION NOTES:
//! * The Run value / `Exec=` value is the quoted absolute app path;
//!   desktop files need quoting for paths with spaces. Name constants are
//!   shared (`MultiTunnel` value name / `multitunnel.desktop` file name).
//! * The desktop file carries no secrets → mode 0644 (only credential
//!   files are 0600 per the global security rule).
//! * `disable` is idempotent: a missing value/file is success.

use std::path::Path;

/// Value name in the HKCU Run key / well-known desktop file stem.
pub(crate) const AUTOSTART_NAME: &str = "MultiTunnel";

/// The value written to the HKCU Run key for `app_path` (quoted).
pub fn run_key_value(app_path: &Path) -> String {
    format!("\"{}\"", app_path.display())
}

/// Body of the XDG autostart desktop file for `app_path`.
pub fn desktop_entry(app_path: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={AUTOSTART_NAME}\n\
         Comment=MultiTunnel all-in-one proxy manager\n\
         Exec=\"{}\"\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        app_path.display()
    )
}

/// Enable launch-on-login for the app at `app_path`.
pub fn autostart_enable(app_path: &Path) -> anyhow::Result<()> {
    platform_enable(app_path)
}

/// Disable launch-on-login (idempotent).
pub fn autostart_disable() -> anyhow::Result<()> {
    platform_disable()
}

/// Is launch-on-login currently enabled?
pub fn autostart_is_enabled() -> bool {
    platform_is_enabled()
}

// ---------------------------------------------------------------------------
// Linux / macOS / everything else: XDG autostart desktop file
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
fn autostart_file() -> std::path::PathBuf {
    crate::dirs::xdg_config_home()
        .join("autostart")
        .join("multitunnel.desktop")
}

#[cfg(not(windows))]
fn platform_enable(app_path: &Path) -> anyhow::Result<()> {
    let file = autostart_file();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&file, desktop_entry(app_path))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644))?;
    }
    tracing::info!(file = %file.display(), path = %app_path.display(), "autostart enabled");
    Ok(())
}

#[cfg(not(windows))]
fn platform_disable() -> anyhow::Result<()> {
    match std::fs::remove_file(autostart_file()) {
        Ok(()) => {
            tracing::info!("autostart disabled");
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(not(windows))]
fn platform_is_enabled() -> bool {
    autostart_file().is_file()
}

// ---------------------------------------------------------------------------
// Windows: HKCU Run key via winreg
// ---------------------------------------------------------------------------

#[cfg(windows)]
const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

#[cfg(windows)]
fn run_key() -> Result<winreg::RegKey, std::io::Error> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(RUN_SUBKEY)?;
    Ok(key)
}

#[cfg(windows)]
fn open_run_key() -> Result<winreg::RegKey, std::io::Error> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    hkcu.open_subkey(RUN_SUBKEY)
}

#[cfg(windows)]
fn platform_enable(app_path: &Path) -> anyhow::Result<()> {
    let value = run_key_value(app_path);
    run_key()?.set_value(AUTOSTART_NAME, &value)?;
    tracing::info!(path = %app_path.display(), "autostart enabled");
    Ok(())
}

#[cfg(windows)]
fn platform_disable() -> anyhow::Result<()> {
    match open_run_key() {
        Ok(key) => match key.delete_value(AUTOSTART_NAME) {
            Ok(()) => {
                tracing::info!("autostart disabled");
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(windows)]
fn platform_is_enabled() -> bool {
    open_run_key()
        .and_then(|key| key.get_value::<String, _>(AUTOSTART_NAME))
        .is_ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_key_value_quotes_the_path() {
        let quoted = run_key_value(Path::new("C:\\Program Files\\MultiTunnel\\multitunnel.exe"));
        assert_eq!(
            quoted,
            "\"C:\\Program Files\\MultiTunnel\\multitunnel.exe\""
        );
        assert_eq!(
            run_key_value(Path::new("/usr/bin/multitunnel")),
            "\"/usr/bin/multitunnel\""
        );
    }

    #[test]
    fn desktop_entry_content_is_valid() {
        let body = desktop_entry(Path::new("/opt/multi tunnel/multitunnel"));
        assert!(body.starts_with("[Desktop Entry]\n"));
        assert!(body.contains("Type=Application"));
        assert!(body.contains("Name=MultiTunnel"));
        assert!(
            body.contains("Exec=\"/opt/multi tunnel/multitunnel\""),
            "quoted Exec"
        );
        assert!(body.contains("X-GNOME-Autostart-enabled=true"));
        assert!(!body.contains("Password"), "never contains credentials");
    }

    #[cfg(not(windows))]
    #[test]
    fn enable_disable_roundtrip_under_xdg_home() {
        use crate::test_support::{env_lock, set_env, temp_root, unset_env};

        let _guard = env_lock();
        let base = temp_root("autostart");
        set_env("XDG_CONFIG_HOME", base.join("config-home"));

        assert!(!autostart_is_enabled(), "fresh home: disabled");
        autostart_enable(Path::new("/usr/local/bin/multitunnel")).expect("enable");

        let file = base
            .join("config-home")
            .join("autostart")
            .join("multitunnel.desktop");
        assert!(file.is_file());
        let body = std::fs::read_to_string(&file).unwrap();
        assert_eq!(body, desktop_entry(Path::new("/usr/local/bin/multitunnel")));
        assert!(autostart_is_enabled());

        autostart_disable().expect("disable");
        assert!(!autostart_is_enabled());
        autostart_disable().expect("disable is idempotent");

        unset_env("XDG_CONFIG_HOME");
    }
}
