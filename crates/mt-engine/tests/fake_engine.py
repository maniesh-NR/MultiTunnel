#!/usr/bin/env python3
"""Fake Xray engine for mt-engine tests (contract: NO real xray in tests).

Usage: python3 fake_engine.py <engine-config.json>

- Reads the SAME config file a real Xray would get, takes the SOCKS
  listen address/port from inbounds[0], binds it and answers no-auth
  SOCKS5 CONNECT (RFC 1928 subset).
- After a successful CONNECT reply it enters ECHO mode: every byte
  received is sent back, so tests can round-trip data "through the
  tunnel" without any real upstream.
- Probe connections (connect + immediate close) are tolerated.
- Exit control (used by the lifecycle tests to simulate an unexpected
  crash): if the file `<config-path>.exit` exists, the engine exits.
  Bind first, then poll, so a stale exit file never masks the port.
"""
import json
import os
import socket
import sys


def recvn(conn, n):
    """Read exactly n bytes or None (EOF / timeout / error)."""
    if n == 0:
        return b""
    buf = b""
    while len(buf) < n:
        try:
            chunk = conn.recv(n - len(buf))
        except Exception:
            return None
        if not chunk:
            return None
        buf += chunk
    return buf


def serve(conn):
    """One SOCKS5 no-auth CONNECT + echo session."""
    try:
        conn.settimeout(2.0)
        head = recvn(conn, 2)
        if head is None or head[0] != 5:
            return
        methods = recvn(conn, head[1])
        if methods is None:
            return
        if 0 not in methods:
            conn.sendall(b"\x05\xff")
            return
        conn.sendall(b"\x05\x00")
        head = recvn(conn, 4)
        if head is None or head[0] != 5:
            return
        _ver, cmd, _rsv, atyp = head
        if cmd != 1:
            conn.sendall(b"\x05\x07\x00\x01" + b"\x00" * 6)  # BIND/UDP -> 07
            return
        if atyp == 1:
            if recvn(conn, 4) is None:
                return
        elif atyp == 3:
            ln = recvn(conn, 1)
            if ln is None or recvn(conn, ln[0]) is None:
                return
        elif atyp == 4:
            if recvn(conn, 16) is None:
                return
        else:
            conn.sendall(b"\x05\x08\x00\x01" + b"\x00" * 6)  # bad ATYP -> 08
            return
        if recvn(conn, 2) is None:
            return
        conn.sendall(b"\x05\x00\x00\x01" + b"\x00" * 6)
        conn.settimeout(10.0)
        while True:
            data = conn.recv(4096)
            if not data:
                break
            conn.sendall(data)
    except Exception as exc:
        sys.stderr.write("fake-engine: conn error: %r\n" % (exc,))
        sys.stderr.flush()
    finally:
        try:
            conn.close()
        except Exception:
            pass


def main():
    if len(sys.argv) < 2:
        sys.stderr.write("usage: fake_engine.py <config.json>\n")
        return 2
    cfg_path = sys.argv[1]
    with open(cfg_path, "r", encoding="utf-8") as f:
        cfg = json.load(f)
    inbound = cfg["inbounds"][0]
    listen = inbound.get("listen", "127.0.0.1")
    port = int(inbound["port"])
    exit_file = cfg_path + ".exit"

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((listen, port))
    srv.listen(16)
    srv.settimeout(0.1)
    sys.stderr.write("fake-engine ready on %s:%d\n" % (listen, port))
    sys.stderr.flush()

    while True:
        try:
            conn, addr = srv.accept()
        except socket.timeout:
            if os.path.exists(exit_file):
                sys.stderr.write("fake-engine: exit file seen; exiting\n")
                sys.stderr.flush()
                # Die INSTANTLY: the listening socket and the process must
                # disappear together, so a port-free check implies the
                # process is reaped (plain interpreter shutdown closes the
                # socket before the process dies, which races supervisors).
                srv.close()
                os._exit(0)
            continue
        except Exception as exc:
            sys.stderr.write("fake-engine: accept error: %r\n" % (exc,))
            sys.stderr.flush()
            return 1
        # Exit file is only checked between connections; tests close
        # their connections before killing via the exit file.
        serve(conn)


if __name__ == "__main__":
    sys.exit(main())
