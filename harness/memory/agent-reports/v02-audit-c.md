# v0.2.0-AUDIT-C (correctness/security, on the recycled Nov-12 tree) — agent-ac1cd5f2
APPROVE. 0 P0 / 3 P1 / 10 P2. Key P1s: (1) connect validates liveness only
via port+probe — a foreign listener on socksPort with a dead own core yields
Ok/Connected + endless restart churn (repro'd empirically via public API);
(2) no Job Object kill-on-close — the xray child survives Ctrl+C (the
documented CLI exit) and can never be stopped afterwards; (3) sysproxy
snapshot write not atomic + corrupt snapshot errs forever while callers
swallow → system proxy permanently stuck (defeats D5 fail-open).
Verified: 52/52 tests, zigbuild green, both exes correct subsystems; 10/10
adversarial SOCKS5 probes against the real socks_probe (partial reads, ATYP
02/03/04 variants, silent server, overflow) — no panics/hangs; zero
unwrap/expect in non-test code; connect order + D5 give-up path exact.
