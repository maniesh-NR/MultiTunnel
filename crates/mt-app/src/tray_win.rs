//! The Windows half of the in-app tray (S15, D25) + the non-Windows stub.
//! All FFI shapes are lifted from the S3-reviewed `src/tray.rs` (D4: raw
//! Win32 via windows-sys, not the tray-icon crate) — see that module's
//! docs for the full safety analysis; the SAFETY comments here mirror it.

#[cfg(windows)]
mod win {
    use std::cell::RefCell;
    use std::sync::mpsc::{Receiver, SyncSender};
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
        DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
        KillTimer, LoadIconW, PostMessageW, PostQuitMessage, RegisterWindowMessageW,
        SetForegroundWindow, SetTimer, SetWindowLongPtrW, TrackPopupMenuEx, TranslateMessage,
        GWLP_WNDPROC, HICON, HMENU, IDI_APPLICATION, LR_DEFAULTCOLOR, MF_CHECKED, MF_GRAYED,
        MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_RIGHTBUTTON, WM_APP,
        WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_TIMER, WS_OVERLAPPED,
    };

    use crate::tray::ffi_helpers::{
        ico_image_bytes, tip_array, to_wide, ICON_BYTES, ICON_MONO_BYTES, ICON_SIZE,
    };
    use crate::tray::{
        action_for_id, assign_ids, QuickAction, QuickEntry, TrayCommand, TrayEvent, TrayIcon,
    };

    /// Tray icon id (NOTIFYICONDATAW.uID) — one icon per window.
    const TRAY_ID: u32 = 1;
    /// Callback message the shell sends for icon events.
    const TRAY_CALLBACK: u32 = WM_APP + 1;
    /// Command-poll timer id + period (the app's commands are rare; 50 ms
    /// is imperceptible and keeps Exit teardown under ~100 ms).
    const TIMER_ID: usize = 1;
    const POLL_PERIOD_MS: u32 = 50;

    /// Window/thread state. `ui` is only ever touched from the message-loop
    /// thread (RefCell is enough — same as the legacy tray).
    struct TrayCtx {
        cmd_rx: Receiver<TrayCommand>,
        event_tx: tokio::sync::mpsc::UnboundedSender<TrayEvent>,
        ui: RefCell<UiState>,
    }

    struct UiState {
        hwnd: HWND,
        /// S16: both icons live for the app's lifetime; `icon` is the one
        /// currently shown (swapped by TrayCommand::Icon).
        icon: HICON,
        /// True only when `icon` came from `CreateIconFromResourceEx` (must
        /// then be `DestroyIcon`d). A `LoadIconW` fallback is a SHARED
        /// system icon — never destroyed.
        icon_owned: bool,
        icon_added: bool,
        /// The monochrome (not-connected) icon + ownership.
        icon_mono: HICON,
        icon_mono_owned: bool,
        /// Which style is currently on the tray (S16).
        icon_style: TrayIcon,
        /// Registered "TaskbarCreated" id (0 = registration failed).
        taskbar_msg: u32,
        last_tip: String,
        /// Current Quick Settings model + its flat id map.
        model: Vec<QuickEntry>,
        ids: Vec<(u16, QuickAction)>,
    }

    thread_local! {
        static CTX: RefCell<Option<&'static TrayCtx>> = const { RefCell::new(None) };
    }

    fn ctx() -> Option<&'static TrayCtx> {
        CTX.with(|c| *c.borrow())
    }

    fn log_err(op: &str) {
        tracing::warn!("tray: {op} failed: {}", std::io::Error::last_os_error());
    }

    /// The message loop thread body: create window → add icon → pump until
    /// WM_QUIT → teardown. Errors before the loop turn into a `None`
    /// (reported by the caller as "tray unavailable").
    pub(crate) fn spawn_impl(
        initial_tooltip: String,
        initial_model: Vec<QuickEntry>,
        initial_icon: TrayIcon,
    ) -> Option<(
        crate::tray::TrayHandle,
        tokio::sync::mpsc::UnboundedReceiver<TrayEvent>,
    )> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::sync_channel(64);
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        // Wake the parent even if the thread panics before signaling.
        let _guard = DropSend(Some(ready_tx.clone()));
        let initial_ids = assign_ids(&initial_model);
        // TrayCtx (RefCell UI state + the command Receiver) is created ON
        // the tray thread and leaked there — it never crosses a thread
        // boundary (no Sync needed); the parent only holds the channels.
        let thread = std::thread::Builder::new()
            .name("mt-tray".into())
            .spawn(move || {
                let ctx: &'static TrayCtx = Box::leak(Box::new(TrayCtx {
                    cmd_rx,
                    event_tx,
                    ui: RefCell::new(UiState {
                        hwnd: std::ptr::null_mut(),
                        icon: std::ptr::null_mut(),
                        icon_owned: false,
                        icon_added: false,
                        icon_mono: std::ptr::null_mut(),
                        icon_mono_owned: false,
                        icon_style: initial_icon,
                        taskbar_msg: 0,
                        last_tip: initial_tooltip,
                        model: initial_model,
                        ids: initial_ids,
                    }),
                }));
                run(ctx, ready_tx)
            });
        match thread {
            Ok(handle) => {
                let ok = ready_rx.recv().unwrap_or(false);
                if ok {
                    HANDLE_SLOT.with(|s| *s.borrow_mut() = Some(handle));
                    Some((crate::tray::TrayHandle(HandleInner { cmd_tx }), event_rx))
                } else {
                    tracing::warn!("tray unavailable: init failed");
                    None
                }
            }
            Err(e) => {
                tracing::warn!("tray unavailable: spawn mt-tray: {e}");
                None
            }
        }
    }

    /// Wake the parent even if the thread panics before signaling: sends
    /// `false` on drop when the sender was not used.
    struct DropSend(Option<SyncSender<bool>>);

    impl Drop for DropSend {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.try_send(false);
            }
        }
    }

    thread_local! {
        /// The tray thread's JoinHandle, held for `HandleInner::shutdown`.
        static HANDLE_SLOT: RefCell<Option<std::thread::JoinHandle<()>>> =
            const { RefCell::new(None) };
    }

    /// The app-facing half (the public wrapper lives in tray.rs).
    pub(crate) struct HandleInner {
        cmd_tx: SyncSender<TrayCommand>,
    }
    impl HandleInner {
        pub(crate) fn send(&self, cmd: TrayCommand) {
            use std::sync::mpsc::TrySendError;
            // Full channel = the tray is wedged; dropping the command is
            // the honest degradation (the app keeps running).
            match self.cmd_tx.try_send(cmd) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    tracing::warn!("tray: command channel closed (thread gone)");
                }
            }
        }

        pub(crate) fn shutdown(self, wait: Duration) {
            let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
            self.cmd_tx.try_send(TrayCommand::Shutdown(ack_tx)).ok();
            if ack_rx.recv_timeout(wait).is_err() {
                tracing::warn!("tray: teardown ack timeout — detaching thread");
            }
            // Join best-effort (the ack usually means the loop already
            // exited; joining reaps the thread's resources).
            HANDLE_SLOT.with(|s| {
                if let Some(h) = s.borrow_mut().take() {
                    let _ = h.join();
                }
            });
        }
    }

    fn run(ctx: &'static TrayCtx, ready: SyncSender<bool>) {
        // Window + icon first; failures go to the parent, and the thread
        // ends without a message loop.
        let hwnd = match create_window() {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("tray: {e}");
                let _ = ready.send(false);
                return;
            }
        };
        CTX.with(|c| *c.borrow_mut() = Some(ctx));
        {
            let mut ui = ctx.ui.borrow_mut();
            ui.hwnd = hwnd;
            let (icon, owned) = load_tray_icon(ICON_BYTES);
            ui.icon = icon;
            ui.icon_owned = owned;
            let (mono, mono_owned) = load_tray_icon(ICON_MONO_BYTES);
            ui.icon_mono = mono;
            ui.icon_mono_owned = mono_owned;
            let tip = ui.last_tip.clone();
            let shown = ui.icon_for(ui.icon_style);
            drop(ui);
            if add_tray_icon(hwnd, shown, &tip).is_err() {
                // SAFETY: hwnd was created on this thread.
                unsafe { DestroyWindow(hwnd) };
                CTX.with(|c| *c.borrow_mut() = None);
                tracing::warn!("tray: Shell_NotifyIconW(NIM_ADD) failed");
                let _ = ready.send(false);
                return;
            }
            ctx.ui.borrow_mut().icon_added = true;
        }
        {
            let mut ui = ctx.ui.borrow_mut();
            // SAFETY: registering a message by name never fails critically;
            // id 0 just disables the explorer-restart recovery.
            ui.taskbar_msg = unsafe { RegisterWindowMessageW(to_wide("TaskbarCreated").as_ptr()) };
            // SAFETY: our own window + timer id; 50 ms period.
            unsafe { SetTimer(hwnd, TIMER_ID, POLL_PERIOD_MS, None) };
        }
        if ready.send(true).is_err() {
            // Parent went away before the ack: tear back down.
            teardown(ctx, hwnd);
            return;
        }

        // Classic pump; WM_QUIT arrives from WM_DESTROY (window died) or
        // the Shutdown command.
        let mut msg = MSG {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: POINT { x: 0, y: 0 },
        };
        loop {
            // SAFETY: msg is a writable, aligned out-slot; the pump only
            // runs on this thread (the procedure was installed on it).
            let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if r <= 0 {
                break;
            }
            // SAFETY: passthrough of a retrieved message.
            unsafe { TranslateMessage(&msg) };
            // SAFETY: passthrough of a translated message.
            unsafe { DispatchMessageW(&msg) };
        }
        teardown(ctx, hwnd);
    }

    /// Final teardown: timer off, icon off, icon handles freed, window
    /// gone (WM_DESTROY already quit the loop).
    fn teardown(ctx: &'static TrayCtx, hwnd: HWND) {
        // SAFETY: our own timer.
        unsafe { KillTimer(hwnd, TIMER_ID) };
        {
            let mut ui = ctx.ui.borrow_mut();
            if ui.icon_added && !hwnd.is_null() {
                let d = nid_base(hwnd);
                // SAFETY: d carries the icon identity; NIM_DELETE removes it.
                let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &d) };
            }
            for (icon, owned) in [(ui.icon, ui.icon_owned), (ui.icon_mono, ui.icon_mono_owned)] {
                if owned && !icon.is_null() {
                    // SAFETY: icon came from CreateIconFromResourceEx.
                    unsafe { DestroyIcon(icon) };
                }
            }
            ui.icon_added = false;
            ui.icon = std::ptr::null_mut();
            ui.icon_mono = std::ptr::null_mut();
        }
    }

    // ---- icon / window ---------------------------------------------------------

    fn load_tray_icon(ico_bytes: &[u8]) -> (HICON, bool) {
        if let Some(bytes) = ico_image_bytes(ico_bytes, ICON_SIZE) {
            // SAFETY: bytes points into our read-only embedded ICO data
            // with exactly the length from the directory entry; 0x00030000
            // is the documented icon-resource version; fIcon=1.
            let icon = unsafe {
                CreateIconFromResourceEx(
                    bytes.as_ptr(),
                    bytes.len() as u32,
                    1,
                    0x0003_0000,
                    ICON_SIZE as i32,
                    ICON_SIZE as i32,
                    LR_DEFAULTCOLOR,
                )
            };
            if !icon.is_null() {
                return (icon, true);
            }
        }
        // SAFETY: IDI_APPLICATION is a system resource id; LoadIconW
        // returns a SHARED icon (never destroyed — owned=false).
        (
            unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) },
            false,
        )
    }

    impl UiState {
        /// The HICON currently representing a style. Falls back to the
        /// colored icon when the mono handle is unavailable (load
        /// failure) — a wrong-color icon beats an invisible one.
        fn icon_for(&self, style: TrayIcon) -> HICON {
            match style {
                TrayIcon::Color => self.icon,
                TrayIcon::Mono if !self.icon_mono.is_null() => self.icon_mono,
                TrayIcon::Mono => self.icon,
            }
        }
    }

    /// Hidden TOP-LEVEL STATIC window, subclassed with our procedure (see
    /// the legacy tray's S3-REV-B P1 analysis: top-level, NOT message-only
    /// — TaskbarCreated must reach it and SetForegroundWindow must be able
    /// to activate it for menu dismissal).
    fn create_window() -> Result<HWND, String> {
        let class = to_wide("STATIC");
        let title = to_wide("MultiTunnelAppTray");
        // SAFETY: hidden top-level window from the predefined STATIC
        // system class; WS_OVERLAPPED without WS_VISIBLE = never shown;
        // both wide strings are NUL-terminated buffers we own.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(format!(
                "CreateWindowExW(tray) failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: the fn-pointer→integer cast is the documented Win32
        // idiom for GWLP_WNDPROC.
        let prev = unsafe {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wndproc as *const () as usize as isize)
        };
        if prev == 0 {
            let e = format!(
                "SetWindowLongPtrW(GWLP_WNDPROC) failed: {}",
                std::io::Error::last_os_error()
            );
            // SAFETY: hwnd was created on this thread.
            unsafe { DestroyWindow(hwnd) };
            return Err(e);
        }
        Ok(hwnd)
    }

    // ---- window procedure ------------------------------------------------------

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if let Some(ctx) = ctx() {
            let taskbar_msg = ctx.ui.borrow().taskbar_msg;
            if taskbar_msg != 0 && msg == taskbar_msg {
                on_taskbar_created(ctx, hwnd);
                return 0;
            }
        }
        match msg {
            WM_TIMER if wparam == TIMER_ID => {
                if let Some(ctx) = ctx() {
                    on_timer(ctx, hwnd);
                }
                0
            }
            WM_COMMAND => {
                let id = (wparam & 0xFFFF) as u16;
                if let Some(ctx) = ctx() {
                    let action = {
                        let ui = ctx.ui.borrow();
                        action_for_id(&ui.ids, id)
                    };
                    if let Some(a) = action {
                        let _ = ctx.event_tx.send((&a).into());
                    }
                }
                0
            }
            WM_DESTROY => {
                // SAFETY: posts WM_QUIT to this thread's loop.
                unsafe { PostQuitMessage(0) };
                0
            }
            TRAY_CALLBACK => match lparam as u32 {
                WM_LBUTTONUP => {
                    if let Some(ctx) = ctx() {
                        let _ = ctx.event_tx.send(TrayEvent::Open);
                    }
                    0
                }
                WM_RBUTTONUP => {
                    if let Some(ctx) = ctx() {
                        show_menu(ctx, hwnd);
                    }
                    0
                }
                _ => {
                    // SAFETY: passthrough.
                    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
                }
            },
            _ => {
                // SAFETY: passthrough.
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
        }
    }

    // ---- handlers ----------------------------------------------------------------

    /// Explorer restarted: re-add the SAME icon identity (the style
    /// currently selected — S16).
    fn on_taskbar_created(ctx: &'static TrayCtx, hwnd: HWND) {
        let (icon, added, tip) = {
            let ui = ctx.ui.borrow();
            (
                ui.icon_for(ui.icon_style),
                ui.icon_added,
                ui.last_tip.clone(),
            )
        };
        if !added || icon.is_null() {
            return;
        }
        if add_tray_icon(hwnd, icon, &tip).is_err() {
            log_err("Shell_NotifyIconW(NIM_ADD after TaskbarCreated)");
        }
    }

    /// Poll the command channel (rare) + refresh the tooltip when the app
    /// sent a new one.
    fn on_timer(ctx: &'static TrayCtx, hwnd: HWND) {
        while let Ok(cmd) = ctx.cmd_rx.try_recv() {
            match cmd {
                TrayCommand::Tooltip(tip) => {
                    let changed = {
                        let mut ui = ctx.ui.borrow_mut();
                        if ui.last_tip == tip {
                            false
                        } else {
                            ui.last_tip = tip.clone();
                            true
                        }
                    };
                    if changed {
                        let mut d = nid_base(hwnd);
                        d.uFlags = NIF_TIP;
                        d.szTip = tip_array(&tip);
                        // SAFETY: NIM_MODIFY only reads d; a missing
                        // icon just returns FALSE.
                        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
                    }
                }
                TrayCommand::Model(model) => {
                    let ids = assign_ids(&model);
                    let mut ui = ctx.ui.borrow_mut();
                    ui.model = model;
                    ui.ids = ids;
                }
                TrayCommand::Icon(style) => {
                    let icon = {
                        let mut ui = ctx.ui.borrow_mut();
                        if ui.icon_style == style {
                            continue;
                        }
                        ui.icon_style = style;
                        ui.icon_for(style)
                    };
                    let mut d = nid_base(hwnd);
                    d.uFlags = NIF_ICON;
                    d.hIcon = icon;
                    // SAFETY: NIM_MODIFY only reads d; a missing icon just
                    // returns FALSE.
                    let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
                }
                TrayCommand::Shutdown(ack) => {
                    // Stop the icon first (best effort), ack, then let
                    // WM_DESTROY quit the loop.
                    {
                        let ui = ctx.ui.borrow();
                        if ui.icon_added && !hwnd.is_null() {
                            let d = nid_base(hwnd);
                            // SAFETY: icon identity; NIM_DELETE.
                            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &d) };
                        }
                    }
                    ctx.ui.borrow_mut().icon_added = false;
                    let _ = ack.send(());
                    // SAFETY: our own window; WM_DESTROY → PostQuitMessage.
                    unsafe { DestroyWindow(hwnd) };
                }
            }
        }
    }

    fn show_menu(ctx: &'static TrayCtx, hwnd: HWND) {
        let model = ctx.ui.borrow().model.clone();
        // SAFETY: fresh popup menu owned by this thread.
        let hmenu = unsafe { CreatePopupMenu() };
        if hmenu.is_null() {
            log_err("CreatePopupMenu");
            return;
        }
        // Ids are handed out in the same depth-first walk order as
        // `assign_ids` (items only), so WM_COMMAND lookups agree.
        let mut next_id: u16 = crate::tray::FIRST_ID;
        append_entries(hmenu, &model, &mut next_id);
        let mut pt = POINT { x: 0, y: 0 };
        // SAFETY: pt is a writable out-slot.
        let have_pos = unsafe { GetCursorPos(&mut pt) } != 0;
        if have_pos {
            // SAFETY: hwnd is our (foreground-able, top-level) window.
            let _ = unsafe { SetForegroundWindow(hwnd) };
        }
        // SAFETY: hmenu valid; modal loop re-enters the procedure safely
        // (no borrow guard is held here).
        let _ = unsafe {
            TrackPopupMenuEx(
                hmenu,
                TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                hwnd,
                std::ptr::null(),
            )
        };
        if have_pos {
            // SAFETY: WM_NULL is the documented post-menu no-op (KB
            // Q135788 second half).
            let _ = unsafe { PostMessageW(hwnd, WM_NULL, 0, 0) };
        }
        // SAFETY: destroys the menu we created.
        unsafe { DestroyMenu(hmenu) };
    }

    /// Append model entries; `next_id` is consumed in the same depth-first
    /// order as [`crate::tray::assign_ids`], keeping menu ids and the
    /// stored id map in lockstep.
    fn append_entries(hmenu: HMENU, entries: &[QuickEntry], next_id: &mut u16) {
        for e in entries {
            let ok = match e {
                QuickEntry::Separator => {
                    // SAFETY: MF_SEPARATOR requires a NULL lpNewItem.
                    unsafe { AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null()) }
                }
                QuickEntry::Header(label) => {
                    let wide = to_wide(label);
                    // SAFETY: grayed rows are unselectable (id 0 unused).
                    unsafe { AppendMenuW(hmenu, MF_STRING | MF_GRAYED, 0, wide.as_ptr()) }
                }
                QuickEntry::Item {
                    label,
                    enabled,
                    checked,
                    ..
                } => {
                    let id = *next_id;
                    *next_id = next_id.wrapping_add(1);
                    let wide = to_wide(label);
                    let mut flags = MF_STRING;
                    if !enabled {
                        flags |= MF_GRAYED;
                    }
                    if *checked {
                        flags |= MF_CHECKED;
                    }
                    // SAFETY: id maps back through action_for_id on
                    // WM_COMMAND.
                    unsafe { AppendMenuW(hmenu, flags, id as usize, wide.as_ptr()) }
                }
                QuickEntry::Submenu { label, items } => {
                    // SAFETY: fresh popup owned by this thread.
                    let sub = unsafe { CreatePopupMenu() };
                    if sub.is_null() {
                        log_err("CreatePopupMenu(submenu)");
                        continue;
                    }
                    append_entries(sub, items, next_id);
                    let wide = to_wide(label);
                    // SAFETY: sub is a valid menu; MF_POPUP makes the id
                    // parameter the submenu HANDLE.
                    let ok = unsafe { AppendMenuW(hmenu, MF_POPUP, sub as usize, wide.as_ptr()) };
                    if ok == 0 {
                        log_err("AppendMenuW(submenu)");
                        // SAFETY: not attached — destroy to avoid a leak.
                        unsafe { DestroyMenu(sub) };
                    }
                    continue;
                }
            };
            if ok == 0 {
                log_err("AppendMenuW");
            }
        }
    }

    // ---- Shell_NotifyIconW glue ---------------------------------------------------

    fn nid_base(hwnd: HWND) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ID,
            ..Default::default()
        }
    }

    fn add_tray_icon(hwnd: HWND, icon: HICON, tip: &str) -> Result<(), String> {
        let mut d = nid_base(hwnd);
        d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        d.uCallbackMessage = TRAY_CALLBACK;
        d.hIcon = icon;
        d.szTip = tip_array(tip);
        // SAFETY: d fully initialized for NIM_ADD.
        if unsafe { Shell_NotifyIconW(NIM_ADD, &d) } != 0 {
            Ok(())
        } else {
            Err(format!(
                "Shell_NotifyIconW(NIM_ADD) failed: {}",
                std::io::Error::last_os_error()
            ))
        }
    }
}

#[cfg(windows)]
pub(crate) use win::{spawn_impl, HandleInner};

#[cfg(not(windows))]
mod stub {
    use crate::tray::{QuickEntry, TrayCommand, TrayEvent, TrayIcon};

    /// Non-Windows: no tray (the app keeps working; close-to-tray
    /// degrades to minimize — see D25 §3).
    pub fn spawn_impl(
        _initial_tooltip: String,
        _initial_model: Vec<QuickEntry>,
        _initial_icon: TrayIcon,
    ) -> Option<(
        crate::tray::TrayHandle,
        tokio::sync::mpsc::UnboundedReceiver<TrayEvent>,
    )> {
        tracing::info!("tray: not available on this platform (Windows only)");
        None
    }

    pub struct HandleInner;

    impl HandleInner {
        pub fn send(&self, _cmd: TrayCommand) {}
        pub fn shutdown(self, _wait: std::time::Duration) {}
    }
}

#[cfg(not(windows))]
pub(crate) use stub::{spawn_impl, HandleInner};
