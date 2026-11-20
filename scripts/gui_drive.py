#!/usr/bin/env python3
"""gui_drive.py — one-shot Xvfb GUI drive rig (S10/S13/S15).

Usage:
    python3 scripts/gui_drive.py <binary> <out_dir> <screen_WxH> <sf> <verbs...>

Verbs (comma-separated list, executed in order):
    wait:<sec>            sleep
    shot:<name>           screenshot to <out_dir>/<name>.png (ffmpeg x11grab)
    click:<x>,<y>         XTEST button-1 press+release at LOGICAL coords
    rclick:<x>,<y>        XTEST button-3 press+release (context menu)
    dclick:<x>,<y>        two clicks 60 ms apart
    type:<text>           XTEST typed ASCII (keymap-level shift handling)
    key:<keysym>          XTEST single keysym (e.g. Return, Escape)
    scroll:<x>,<y>        one XTEST wheel-down (button 5) at coords
                          (append ",up" for button 4, ",N" to repeat)
    resize:<W>x<H>        XConfigureWindow on the app window (no WM runs)
    alive                 assert the app process is still alive (exit 4 if not)
    gone                  assert the app process EXITED (exit 5 if not)

Exit 0 = all verbs done + the alive/gone assertions passed. Everything is
one-shot: the whole process tree (Xvfb, app, ffmpeg) is torn down at the
end (the sandbox kills daemons between tool calls anyway).
"""

import os
import shutil
import signal
import subprocess
import sys
import time

sys.path.insert(0, "/home/z/my-project/tools/pypkgs-xlib")

XKBLIBS = "/tmp/xkblibs/usr/lib/x86_64-linux-gnu"
DISPLAY = ":99"

KEYSYM_NAMES = {
    "Return": 0xFF0D,
    "Escape": 0xFF1B,
    "BackSpace": 0xFF08,
    "Tab": 0xFF09,
    "space": 0x20,
}

SHIFTED = {
    "~": "`", "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6",
    "&": "7", "*": "8", "(": "9", ")": "0", "_": "-", "+": "=",
    "{": "[", "}": "]", ":": ";", '"': "'", "<": ",", ">": ".", "?": "/",
    "|": "\\",
}


def main() -> int:
    binary, out_dir, screen, sf, verbs = (
        sys.argv[1],
        sys.argv[2],
        sys.argv[3],
        sys.argv[4],
        sys.argv[5:],
    )
    os.makedirs(out_dir, exist_ok=True)
    shutil.rmtree("/tmp/.X11-unix/X99", ignore_errors=True)

    xvfb = subprocess.Popen(
        ["Xvfb", DISPLAY, "-screen", "0", f"{screen}x24", "-nolisten", "tcp"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(1.2)

    env = dict(os.environ)
    env["DISPLAY"] = DISPLAY
    env["LD_LIBRARY_PATH"] = XKBLIBS
    env["WINIT_X11_SCALE_FACTOR"] = sf
    env["RUST_LOG"] = "info"
    app = subprocess.Popen(
        [binary], env=env, stdout=open(f"{out_dir}/app.log", "w"),
        stderr=subprocess.STDOUT,
    )
    time.sleep(3.0)

    # XTEST driver -----------------------------------------------------------
    from Xlib import X, display as xdisplay
    from Xlib.ext import xtest

    d = xdisplay.Display(DISPLAY)
    root = d.screen().root

    def find_app_window():
        """The REAL app window: winit also creates an UNMAPPED helper
        window ('winit window') under Xvfb — SetInputFocus on an unviewable
        window is a BadMatch protocol error. Match by WM_NAME + viewable."""
        best = None
        for child in root.query_tree().children:
            try:
                name = child.get_wm_name() or ""
                viewable = child.get_attributes().map_state == X.IsViewable
            except Exception:
                continue
            if viewable and ("MultiTunnel" in name or name):
                best = child
                if "MultiTunnel" in name:
                    return child
        return best

    app_win = find_app_window()

    def focus():
        nonlocal app_win
        if app_win is None:
            app_win = find_app_window()
        if app_win is not None:
            app_win.configure(stack_mode=X.Above)
            # explicit input focus (no WM under Xvfb — never granted
            # otherwise)
            d.set_input_focus(app_win, X.RevertToPointerRoot, X.CurrentTime)
        d.flush()

    def click(x, y):
        xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y))
        d.flush()
        time.sleep(0.05)
        xtest.fake_input(d, X.ButtonPress, 1)
        d.sync()
        time.sleep(0.06)
        xtest.fake_input(d, X.ButtonRelease, 1)
        d.sync()
        time.sleep(0.15)

    def rclick(x, y):
        # S16: button-3 press+release (titlebar context menu).
        xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y))
        d.flush()
        time.sleep(0.05)
        xtest.fake_input(d, X.ButtonPress, 3)
        d.sync()
        time.sleep(0.06)
        xtest.fake_input(d, X.ButtonRelease, 3)
        d.sync()
        time.sleep(0.15)

    def key(code, press):
        xtest.fake_input(d, X.KeyPress if press else X.KeyRelease, code)
        d.sync()

    def type_text(t):
        for ch in t:
            base, shifted = ch, False
            if ch.isupper():
                base, shifted = ch.lower(), True
            elif ch in SHIFTED:
                base, shifted = SHIFTED[ch], True
            code = d.keysym_to_keycode(ord(base))
            if shifted:
                shift_code = d.keysym_to_keycode(0xFFE1)
                key(shift_code, True)
            key(code, True)
            time.sleep(0.02)
            key(code, False)
            if shifted:
                key(shift_code, False)
            time.sleep(0.03)

    def press_named(name):
        code = d.keysym_to_keycode(
            KEYSYM_NAMES.get(name, ord(name) if len(name) == 1 else 0xFF1B)
        )
        key(code, True)
        time.sleep(0.02)
        key(code, False)

    def shot(name):
        w, h = screen.split("x")
        subprocess.run(
            [
                "ffmpeg", "-y", "-loglevel", "error", "-f", "x11grab",
                "-video_size", f"{w}x{h}", "-i", DISPLAY, "-frames:v", "1",
                f"{out_dir}/{name}.png",
            ],
            check=False,
        )

    def scroll(x, y, up=False, n=1):
        # XTEST wheel needs generous timings under Xvfb: a 0.2 s settle
        # after the warp, 80 ms hold, 200 ms between clicks (fast
        # press/release pairs get coalesced and lost).
        xtest.fake_input(d, X.MotionNotify, x=int(x), y=int(y))
        d.flush()
        time.sleep(0.2)
        for _ in range(n):
            xtest.fake_input(d, X.ButtonPress, 4 if up else 5)
            d.sync()
            time.sleep(0.08)
            xtest.fake_input(d, X.ButtonRelease, 4 if up else 5)
            d.sync()
            time.sleep(0.2)

    code = 0
    try:
        focus()
        for v in verbs:
            op, _, arg = v.partition(":")
            if op == "wait":
                time.sleep(float(arg))
            elif op == "shot":
                shot(arg)
            elif op == "click":
                x, y = arg.split(",")
                focus()
                click(int(x), int(y))
            elif op == "rclick":
                x, y = arg.split(",")
                focus()
                rclick(int(x), int(y))
            elif op == "dclick":
                x, y = arg.split(",")
                focus()
                click(int(x), int(y))
                click(int(x), int(y))
            elif op == "type":
                focus()
                type_text(arg)
            elif op == "key":
                focus()
                press_named(arg)
            elif op == "scroll":
                parts = arg.split(",")
                up = len(parts) > 2 and parts[2] == "up"
                n = int(parts[3]) if len(parts) > 3 else 1
                focus()
                scroll(int(parts[0]), int(parts[1]), up, n)
            elif op == "resize":
                if app_win is not None:
                    w, h = arg.split("x")
                    app_win.configure(width=int(w), height=int(h))
                    d.flush()
                    time.sleep(0.6)
                    focus()
            elif op == "alive":
                if app.poll() is not None:
                    print(f"ALIVE-ASSERT-FAILED rc={app.returncode}")
                    code = 4
            elif op == "gone":
                for _ in range(40):
                    if app.poll() is not None:
                        break
                    time.sleep(0.25)
                if app.poll() is None:
                    print("GONE-ASSERT-FAILED still running")
                    code = 5
            elif op == "focus":
                focus()
    finally:
        time.sleep(0.4)
        for p in (app, xvfb):
            if p.poll() is None:
                p.send_signal(signal.SIGTERM)
        time.sleep(0.6)
        for p in (app, xvfb):
            if p.poll() is None:
                p.kill()
    return code


if __name__ == "__main__":
    raise SystemExit(main())
