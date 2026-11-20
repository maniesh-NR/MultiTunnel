//! mt-app's in-app notification-area tray (S15, D25): a persistent icon
//! while the app is open or minimized-to-tray, with the Quick Settings
//! menu on right-click.
//!
//! CONTRACT (mirrors the S3-proven `src/tray.rs` shapes — raw Win32 via
//! windows-sys per D4, NOT the tray-icon crate):
//! - [`spawn`] creates the icon and returns `(TrayHandle, Receiver)`; the
//!   handle sends [`TrayCommand`]s (tooltip/model/shutdown) and the
//!   receiver yields [`TrayEvent`]s (open/connect/disconnect/server/exit).
//!   The tray thread owns a HIDDEN TOP-LEVEL STATIC window (subclassed via
//!   `GWLP_WNDPROC` — top-level, NOT message-only, so `TaskbarCreated`
//!   reaches it and the pre-menu `SetForegroundWindow` works), polls the
//!   command channel on a 50 ms `WM_TIMER`, and tears down cleanly
//!   (`NIM_DELETE` + `DestroyIcon` + ack) on [`TrayCommand::Shutdown`].
//! - LEFT click → [`TrayEvent::Open`] (restore the window). RIGHT click →
//!   the Quick Settings menu (KB Q135788 dismissal pattern).
//! - The menu is built FRESH from the last [`TrayCommand::Model`] at every
//!   open (pure, host-testable model — see [`quick_menu`]); selections map
//!   back through [`action_for_id`] and ride the event channel to the app,
//!   which reuses its existing Message paths (no new service calls).
//! - Non-Windows: [`spawn`] returns `None` (stub) — the GUI keeps working;
//!   `TrayHandle` exists as an uninhabited type so app state compiles.
//!
//! Unsafe-FFI review protocol (D6, same as src/tray.rs): every `unsafe`
//! block carries a `SAFETY:` comment; all logic that can be pure lives in
//! the host-testable helpers below and is unit tested on Linux.

use mt_service::{ProfileView, ServiceState, StatusSnapshot};

/// The Quick Settings menu model (pure; built by the app from live state,
/// rendered by the tray thread). [`assign_ids`] flattens it into the id
/// space `TrackPopupMenuEx` reports back via `WM_COMMAND`.
#[derive(Debug, Clone, PartialEq)]
pub enum QuickEntry {
    /// Horizontal rule.
    Separator,
    /// Grayed, unselectable info line (the header).
    Header(String),
    /// Selectable action.
    Item {
        label: String,
        action: QuickAction,
        enabled: bool,
        /// Radio-style check mark.
        checked: bool,
    },
    /// Submenu (the server list).
    Submenu {
        label: String,
        items: Vec<QuickEntry>,
    },
}

/// What a menu selection asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickAction {
    Connect,
    Disconnect,
    /// Make this profile id active (reconnects, like the GUI picker).
    Server(String),
    /// Restore/focus the main window.
    Open,
    /// Really exit (bypasses close-to-tray).
    Exit,
}

/// Id space layout: the header and separators are unselectable (id 0);
/// items get consecutive ids starting at 1, submenus flattened after
/// their parent. `u16` is exactly `WM_COMMAND`'s LOWORD(wParam).
pub(crate) const FIRST_ID: u16 = 1;

/// The Quick Settings menu from live app state. The shape is STABLE
/// (pinned by tests): header, state-appropriate connect/disconnect,
/// server submenu (radio-checked active, labels are the GUI's profile
/// tags), Open, Exit.
pub fn quick_menu(
    state: Option<ServiceState>,
    profiles: &[ProfileView],
    active_tag: &str,
) -> Vec<QuickEntry> {
    let state = state.unwrap_or(ServiceState::Disconnected);
    let connected = state == ServiceState::Connected;
    let busy = state == ServiceState::Connecting;
    let header = match state {
        ServiceState::Connected => "MultiTunnel — connected",
        ServiceState::Connecting => "MultiTunnel — connecting…",
        ServiceState::Failed => "MultiTunnel — connection failed",
        ServiceState::Disconnected => "MultiTunnel — disconnected",
    };
    let mut items = vec![
        QuickEntry::Header(header.to_string()),
        QuickEntry::Separator,
    ];
    // One state-appropriate item; disabled while a transition is in
    // flight (the same rule the legacy tray's menu_plan applies).
    if connected {
        items.push(QuickEntry::Item {
            label: "Disconnect".to_string(),
            action: QuickAction::Disconnect,
            enabled: !busy,
            checked: false,
        });
    } else {
        items.push(QuickEntry::Item {
            label: "Connect".to_string(),
            action: QuickAction::Connect,
            enabled: !busy,
            checked: false,
        });
    }
    if !profiles.is_empty() {
        let servers = profiles
            .iter()
            .map(|p| QuickEntry::Item {
                label: p.tag.clone(),
                action: QuickAction::Server(p.id.clone()),
                enabled: true,
                checked: p.tag == active_tag,
            })
            .collect();
        items.push(QuickEntry::Submenu {
            label: "Server".to_string(),
            items: servers,
        });
    }
    items.push(QuickEntry::Separator);
    items.push(QuickEntry::Item {
        label: "Open MultiTunnel".to_string(),
        action: QuickAction::Open,
        enabled: true,
        checked: false,
    });
    items.push(QuickEntry::Item {
        label: "Exit".to_string(),
        action: QuickAction::Exit,
        enabled: true,
        checked: false,
    });
    items
}

/// Assign the flat `(id, action)` map for a model (ids are stable for a
/// given model shape; the tray stores this alongside the model). Disabled
/// items keep their slot (ids stay stable when only enablement changes);
/// grayed rows can never be selected, so their ids simply never fire.
pub fn assign_ids(model: &[QuickEntry]) -> Vec<(u16, QuickAction)> {
    fn walk(entries: &[QuickEntry], out: &mut Vec<QuickAction>) {
        for e in entries {
            match e {
                QuickEntry::Separator | QuickEntry::Header(_) => {}
                QuickEntry::Item { action, .. } => out.push(action.clone()),
                QuickEntry::Submenu { items, .. } => walk(items, out),
            }
        }
    }
    let mut flat = Vec::new();
    walk(model, &mut flat);
    flat.into_iter()
        .enumerate()
        .map(|(i, action)| (FIRST_ID + i as u16, action))
        .collect()
}

/// Resolve a `WM_COMMAND` id back to its action (None for 0/unknown).
pub fn action_for_id(ids: &[(u16, QuickAction)], id: u16) -> Option<QuickAction> {
    ids.iter().find(|(i, _)| *i == id).map(|(_, a)| a.clone())
}

/// Tray tooltip (NIF_TIP caps at 128 UTF-16 units incl. NUL; the caller
/// may pass any string — the FFI side truncates safely).
pub fn tooltip(state: ServiceState, active_tag: &str) -> String {
    let word = match state {
        ServiceState::Connected => "connected",
        ServiceState::Connecting => "connecting…",
        ServiceState::Failed => "connection failed",
        ServiceState::Disconnected => "disconnected",
    };
    if active_tag.is_empty() {
        format!("MultiTunnel — {word}")
    } else {
        format!("MultiTunnel — {word} [{active_tag}]")
    }
}

/// True when the tray event stream must be wired (state derived, not
/// cfg'd, so the decision is unit-testable on every platform).
pub fn tray_available() -> bool {
    cfg!(windows)
}

/// Build the menu model from a status snapshot + profile list (the exact
/// input the app has at hand on every tick). `active_tag` is what the
/// snapshot reports (the GUI dropdown matches on it too).
pub fn model_from(status: Option<&StatusSnapshot>, profiles: &[ProfileView]) -> Vec<QuickEntry> {
    let state = status.map(|s| s.state);
    let active_tag = status
        .and_then(|s| s.active_profile.clone())
        .unwrap_or_default();
    quick_menu(state, profiles, &active_tag)
}

// ---- commands / events (cross-thread API) -----------------------------------

/// App → tray.
#[derive(Debug)]
pub enum TrayCommand {
    /// Replace the tooltip (sent on tick, only when the text changed).
    Tooltip(String),
    /// Replace the Quick Settings model.
    Model(Vec<QuickEntry>),
    /// Tear down (NIM_DELETE + ack on the returned channel).
    Shutdown(std::sync::mpsc::SyncSender<()>),
}

/// Tray → app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// Left click / "Open MultiTunnel" — restore + focus the window.
    Open,
    Connect,
    Disconnect,
    Server(String),
    /// "Exit" — really exit (bypasses close-to-tray).
    Exit,
}

impl From<&QuickAction> for TrayEvent {
    fn from(a: &QuickAction) -> Self {
        match a {
            QuickAction::Connect => TrayEvent::Connect,
            QuickAction::Disconnect => TrayEvent::Disconnect,
            QuickAction::Server(id) => TrayEvent::Server(id.clone()),
            QuickAction::Open => TrayEvent::Open,
            QuickAction::Exit => TrayEvent::Exit,
        }
    }
}

// ---- embedded icon (shared with the legacy tray: same assets/icon.ico) ------
// FFI-only helpers: used by the win module and the host tests below
// (same cfg as the legacy src/tray.rs).
#[cfg(any(windows, test))]
pub(crate) mod ffi_helpers {
    /// The app logo as a multi-size ICO (see `scripts/make_logo_assets.py`).
    pub(crate) static ICON_BYTES: &[u8] = include_bytes!("../../../assets/icon.ico");

    /// Preferred icon edge for the tray slot (the ICO carries 16/32/48/256).
    pub(crate) const ICON_SIZE: u32 = 32;

    /// One ICO directory entry (bounds-checked in [`ico_entries`]).
    pub(crate) struct IcoEntry {
        pub width: u32,
        pub height: u32,
        pub offset: u32,
        pub bytes: u32,
    }

    /// Parse the ICO directory (ICONDIR + ICONDIRENTRY array). Rejects empty
    /// files, non-icon types, bogus counts and entries pointing outside the
    /// file (fail closed).
    pub(crate) fn ico_entries(ico: &[u8]) -> Option<Vec<IcoEntry>> {
        if ico.len() < 6 {
            return None;
        }
        let reserved = u16::from_le_bytes([ico[0], ico[1]]);
        let kind = u16::from_le_bytes([ico[2], ico[3]]);
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        if reserved != 0 || kind != 1 || count == 0 || ico.len() < 6 + 16 * count {
            return None;
        }
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let e = &ico[6 + 16 * i..6 + 16 * i + 16];
            let w = e[0] as u32;
            let h = e[1] as u32;
            let bytes = u32::from_le_bytes([e[8], e[9], e[10], e[11]]);
            let offset = u32::from_le_bytes([e[12], e[13], e[14], e[15]]);
            let end = offset.checked_add(bytes)?;
            if end as usize > ico.len() {
                return None;
            }
            out.push(IcoEntry {
                width: if w == 0 { 256 } else { w },
                height: if h == 0 { 256 } else { h },
                offset,
                bytes,
            });
        }
        Some(out)
    }

    /// RT_ICON payload for the preferred square size (exact match, else the
    /// closest fit, else the largest) — the bytes
    /// `CreateIconFromResourceEx` wants (NOT the directory header).
    pub(crate) fn ico_image_bytes(ico: &[u8], preferred: u32) -> Option<&[u8]> {
        let entries = ico_entries(ico)?;
        let best = entries
            .iter()
            .filter(|e| e.width == e.height)
            .min_by_key(|e| {
                let d = e.width.abs_diff(preferred);
                // Prefer >= preferred on ties (sharper downscale than up).
                (d, if e.width < preferred { 1 } else { 0 })
            })
            .or_else(|| entries.iter().max_by_key(|e| e.width))?;
        let s = best.offset as usize;
        let e = s.checked_add(best.bytes as usize)?;
        ico.get(s..e)
    }

    /// NUL-terminated UTF-16 for the FFI.
    pub(crate) fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Tooltip as the fixed NOTIFYICONDATAW.szTip array (128 units, always
    /// NUL-terminated; a surrogate pair that would straddle the terminator
    /// is dropped whole — a lone lead surrogate is invalid UTF-16).
    pub(crate) fn tip_array(s: &str) -> [u16; 128] {
        let mut out = [0u16; 128];
        let mut units = s.encode_utf16().peekable();
        let mut i = 0;
        while i < 127 {
            let Some(u) = units.next() else { break };
            if (0xD800..0xDC00).contains(&u) {
                // Lead surrogate: place the pair only if BOTH units fit in
                // the writable window (0..=126); the trail always follows
                // from Rust's encoder.
                if i + 1 < 127 {
                    if let Some(t) = units.next() {
                        out[i] = u;
                        out[i + 1] = t;
                        i += 2;
                        continue;
                    }
                }
                // Pair would straddle the terminator (or no trail — never
                // from encode_utf16): drop it whole and stop.
                break;
            }
            out[i] = u;
            i += 1;
        }
        out
    }
}

// ---- public spawn API ---------------------------------------------------------

/// Spawn the tray (Windows) or report it unavailable (stub elsewhere).
/// The tooltip/model are seeded so the icon is correct before the app's
/// first tick lands.
pub fn spawn(
    initial_tooltip: String,
    initial_model: Vec<QuickEntry>,
) -> Option<(TrayHandle, tokio::sync::mpsc::UnboundedReceiver<TrayEvent>)> {
    crate::tray_win::spawn_impl(initial_tooltip, initial_model)
}

/// App-side handle: sends commands; joins on drop-free explicit shutdown.
pub struct TrayHandle(pub(crate) crate::tray_win::HandleInner);

impl TrayHandle {
    pub fn send(&self, cmd: TrayCommand) {
        self.0.send(cmd)
    }

    /// Best-effort teardown: asks the thread to exit and waits up to
    /// `wait`; on timeout the thread is detached (the process is exiting
    /// — the icon would be cleaned by Explorer on hover, but the ack
    /// path makes that a non-issue in practice).
    pub fn shutdown(self, wait: std::time::Duration) {
        self.0.shutdown(wait)
    }
}

// ---- tests (host: the pure model, ids, tooltip, ICO parse) -------------------

#[cfg(test)]
mod tests {
    use super::ffi_helpers::{ico_entries, ico_image_bytes, tip_array, ICON_BYTES, ICON_SIZE};
    use super::*;

    fn profile(id: &str, name: &str) -> ProfileView {
        ProfileView {
            id: id.to_string(),
            tag: name.to_string(),
            protocol: "vless".to_string(),
            endpoint: "203.0.113.1:443".to_string(),
            remark: String::new(),
            active: false,
        }
    }

    fn item_label(e: &QuickEntry) -> Option<&str> {
        match e {
            QuickEntry::Item { label, .. } => Some(label),
            QuickEntry::Submenu { label, .. } => Some(label),
            _ => None,
        }
    }

    #[test]
    fn menu_disconnected_shows_connect() {
        let m = quick_menu(Some(ServiceState::Disconnected), &[], "");
        assert_eq!(item_label(&m[2]), Some("Connect"));
        assert_eq!(item_label(m.last().unwrap()), Some("Exit"));
        let header = match &m[0] {
            QuickEntry::Header(h) => h.clone(),
            _ => panic!(),
        };
        assert_eq!(header, "MultiTunnel — disconnected");
    }

    #[test]
    fn menu_connected_shows_disconnect_and_servers() {
        let profiles = vec![profile("a", "VM-1"), profile("b", "VM-2")];
        let m = quick_menu(Some(ServiceState::Connected), &profiles, "VM-2");
        assert_eq!(item_label(&m[2]), Some("Disconnect"));
        let QuickEntry::Submenu { label, items } = &m[3] else {
            panic!("expected server submenu");
        };
        assert_eq!(label, "Server");
        // radio check on the ACTIVE profile only (matched by tag, like
        // the GUI dropdown)
        let checks: Vec<bool> = items
            .iter()
            .map(|e| matches!(e, QuickEntry::Item { checked: true, .. }))
            .collect();
        assert_eq!(checks, vec![false, true]);
    }

    #[test]
    fn menu_transition_disables_the_toggle() {
        let m = quick_menu(Some(ServiceState::Connecting), &[], "");
        let QuickEntry::Item { enabled, .. } = &m[2] else {
            panic!();
        };
        assert!(!enabled);
        let m2 = quick_menu(Some(ServiceState::Connected), &[], "");
        let QuickEntry::Item { enabled, .. } = &m2[2] else {
            panic!();
        };
        assert!(enabled);
    }

    #[test]
    fn menu_failed_is_disconnected_shape_with_honest_header() {
        let m = quick_menu(Some(ServiceState::Failed), &[], "");
        assert_eq!(item_label(&m[2]), Some("Connect"));
        assert!(matches!(&m[0], QuickEntry::Header(h) if h.contains("failed")));
    }

    #[test]
    fn ids_roundtrip_and_skip_zero() {
        let profiles = vec![profile("a", "VM-1")];
        let m = quick_menu(Some(ServiceState::Connected), &profiles, "VM-1");
        let ids = assign_ids(&m);
        assert!(ids.iter().all(|(i, _)| *i >= FIRST_ID));
        // 1 = Disconnect, 2 = server "a", 3 = Open, 4 = Exit
        assert_eq!(ids[0].1, QuickAction::Disconnect);
        assert_eq!(ids[1].1, QuickAction::Server("a".into()));
        assert_eq!(ids[2].1, QuickAction::Open);
        assert_eq!(ids[3].1, QuickAction::Exit);
        assert_eq!(action_for_id(&ids, 0), None);
        assert_eq!(action_for_id(&ids, 999), None);
        assert_eq!(action_for_id(&ids, 3), Some(QuickAction::Open));
    }

    #[test]
    fn tooltip_states() {
        assert_eq!(
            tooltip(ServiceState::Connected, "VM-1"),
            "MultiTunnel — connected [VM-1]"
        );
        assert_eq!(
            tooltip(ServiceState::Disconnected, ""),
            "MultiTunnel — disconnected"
        );
        assert_eq!(
            tooltip(ServiceState::Connecting, ""),
            "MultiTunnel — connecting…"
        );
    }

    #[test]
    fn tip_array_never_overruns_or_splits_surrogates() {
        let mut a = tip_array("");
        assert_eq!(a[0], 0);
        // A 200-unit string caps at 127 + NUL.
        let long = "x".repeat(200);
        a = tip_array(&long);
        assert_eq!(a[126], b'x' as u16);
        assert_eq!(a[127], 0);
        // Surrogate pair at the boundary: the last unit must not be a lone
        // lead surrogate — the encoder drops the pair, keeping a NUL.
        let boundary = format!(
            "{}{}", /* 126 ascii + 1 astral char */
            "x".repeat(126),
            "𝕏"
        );
        a = tip_array(&boundary);
        let last = a[126];
        assert!(last == 0 || !(0xD800..0xDC00).contains(&last));
    }

    #[test]
    fn to_wide_is_nul_terminated() {
        let w = super::ffi_helpers::to_wide("MultiTunnel");
        assert_eq!(*w.last().unwrap(), 0);
        assert_eq!(w.len(), "MultiTunnel".encode_utf16().count() + 1);
    }

    #[test]
    fn ico_embedded_asset_parses_with_32_entry() {
        let b = ico_image_bytes(ICON_BYTES, ICON_SIZE).expect("32px entry");
        // BMP-format RT_ICON payload starts with a 40-byte BITMAPINFOHEADER
        // whose biWidth == 32 and biHeight == 64 (XOR + AND masks).
        assert!(b.len() > 40);
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 32);
        assert_eq!(u32::from_le_bytes(b[8..12].try_into().unwrap()), 64);
    }

    #[test]
    fn ico_rejects_bad_directories() {
        assert!(ico_entries(b"").is_none());
        assert!(ico_entries(&[0, 0, 0, 0, 1, 0]).is_none()); // count without entries
                                                             // entry pointing past EOF
        let mut bogus = vec![0, 0, 1, 0, 1, 0];
        bogus.extend_from_slice(&[32, 32, 0, 0, 1, 0, 32, 0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0]);
        assert!(ico_entries(&bogus).is_none());
    }

    #[test]
    fn model_from_snapshot_states() {
        // None status → disconnected, Connect offered.
        let m = model_from(None, &[]);
        assert_eq!(item_label(&m[2]), Some("Connect"));
    }
}
