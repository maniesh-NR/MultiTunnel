# S1-REV-C (correctness/security) — agent-df58def3
APPROVE. 0 P0 / 2 P1 / 5 P2. Key: credential file perms (P1, fixed),
on/off leniency (P1, fixed). parse_status probed adversarially via scratch
crate in /tmp — no panics, UTF-8-safe, no regex blowup, CRLF fine.
