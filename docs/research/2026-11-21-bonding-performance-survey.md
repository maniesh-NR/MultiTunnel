# Bonding & tunneling performance — literature survey (S18, 2026-11-21)

Scope: what prominent multipath-transport research says about improving
throughput, latency and stability on heterogeneous paths (Wi-Fi +
Ethernet + cellular — exactly MultiTunnel's bonded-carrier setting), and
how each finding maps onto this codebase. Two findings shipped in
v0.12.0 (BLEST tie-guard, ECF completion scheduling); the rest is a
prioritized roadmap with the concrete mechanism we would build.

MultiTunnel's stream bonding (mt-bond) is a user-space analog of MPTCP:
frames of one virtual stream are spread across N carrier TCP tunnels and
re-sequenced by mt-merge-server. The MPTCP scheduler literature transfers
almost 1:1 because the failure modes are the same — head-of-line (HoL)
blocking at the receiver, one-backup-all-backup coupling, and scheduling
onto a path whose send buffer is full.

## 1. Shipped in v0.12.0

### 1.1 BLEST — avoid scheduling onto a backed-up path (Ferlin et al., 2016)

Paper: *BLEST: Blocking Estimation-based MPTCP Scheduler for Heterogeneous
Networks*, IFIP Networking 2016 (Ö. Alay, T. Narten, et al. group;
S. Ferlin first author).
https://ieeexplore.ieee.org/document/7497206
https://www.computer.org/csdl/proceedings-article/ifip-networking/2016/07497206/12OmNxisQT9

Finding: the default MPTCP scheduler keeps filling the send window of the
slow path whenever the fast path's window is momentarily full; the
receiver then waits on the slow path's straggler segment — HoL blocking —
and aggregate throughput collapses below the single-path baseline. BLEST
estimates whether sending on a subflow will block and, if so, buffers
briefly on the faster subflow instead.

MultiTunnel mapping (IMPLEMENTED, `mt-bond/src/endpoint.rs`): among
carriers TIED at the minimum relative-load score, a carrier whose writer
queue needs > `BALANCED_DRAIN_CAP_MS` (250 ms) to drain at its
peer-reported delivery rate is skipped — the frame rides an empty
same-score carrier instead of queuing behind a backlog that stalls the
merge daemon's reorder buffer. The refinement only narrows the tied set:
a deep queue alone never disqualifies the score leader (the S14 lesson —
queue depth is not a saturation signal; relative load is).

### 1.2 ECF — schedule by earliest completion, not raw RTT (Lim et al., 2017)

Paper: *ECF: An MPTCP Path Scheduler to Manage Heterogeneous Paths*
(Y. Lim, E. M. Nahum, et al., 2017).
https://dl.acm.org/doi/10.1145/3143314.3078552
(Cited by ~358.)

Finding: the classic minRTT scheduler piles every segment onto the
lowest-RTT path regardless of its congestion window / backlog; ECF
computes the expected completion time of a segment per path from RTT and
available window and picks the minimum — markedly better fast-path
utilization and completion times on Wi-Fi+LTE style asymmetry.

MultiTunnel mapping (IMPLEMENTED): `lowest-latency` now means earliest
completion — `rtt_ms + 1000 × queued / max(deliver_bps, probe floor)`
— whenever delivery-rate reports exist (S14 peer reporting). A 20 ms
carrier with a 500 ms backlog correctly loses to an empty 60 ms one;
without reports the policy stays the S11 sampled-RTT behavior.

## 2. Roadmap (prioritized, with the concrete mechanism)

### 2.1 Redundant mode for interactive streams (Speedify 17 evidence; DAPS/DaMPS lineage)

Speedify's Redundant Mode sends each packet on every connection
simultaneously (up to 3) — first arrival wins; v17 added stream detection
so live streams get priority inside that mode, plus faster failover
detection. https://speedify.com/blog/release-notes/introducing-speedify-17
Academic lineage: deadline-aware schedulers replicate time-critical data
— DaMPS (CUHK) reports 27–149% deadline-adherence gains at comparable
throughput: http://www.cs.cuhk.hk/~cslui/PUBLICATION/DaMPS.pdf

Mechanism for mt-bond: a third bond mode ("redundant") — `send_data`
broadcasts the frame on ALL live carriers; the merge daemon's reorder
window already deduplicates by sequence number, so the change is almost
entirely on the client side. Bandwidth cost = N×; restrict to a
per-session opt-in (the SOCKS inbound marks streams, e.g. by target
port or a "live" profile toggle). Highest user-visible win for calls.

### 2.2 FEC parity frames across carriers (Speedify-style, Raptor-ish)

Mechanism: every K data frames, emit one XOR (or systematic Reed–Solomon
block) frame scheduled round-robin; the reorder buffer recovers a lost
frame from parity instead of stalling until retransmission. Works with
the existing frame sequencing; needs a frame-type flag and a small
parity encoder (XOR over equal-length padded frames is enough for
K≤8). Removes tail-latency spikes from single-carrier loss bursts
without duplicating all traffic.

### 2.3 Reinjection of stalled frames (MPTCP's reinject-on-timeout)

mt-bond already tracks per-session unacked frames (seq → carrier,
bytes). Mechanism: when a carrier is judged dead/very-slow (writer
drain > several seconds), re-enqueue its unacked frames on the best
other carrier (dedupe by seq at the daemon's reorder window). Today a
dying carrier's in-flight frames stall the session until the 10 s stall
timeout closes it; reinjection converts that hard failure into a
sub-second blip.

### 2.4 Delay-aware coupled rate management (wVegas / BALIA line)

The coupled-CC family (LIA → OLIA → BALIA, plus delay-based wVegas)
exists so one multipath flow does not compete with itself across paths —
the aggregate converges instead of flapping capacity between subflows.
Survey of the four Linux algorithms: arXiv 1812.03210
(https://arxiv.org/abs/1812.03210); a 2024 IEEE comparison finds BALIA
and wVegas the most responsive under congestion
(https://ieeexplore.ieee.org/document/10783253).

MultiTunnel analog: the S14 assign/deliver score already behaves like a
crude coupled controller (equilibrium = capacity-proportional split).
The literature suggests two refinements: (a) include the EWMA RTT trend
(wVegas-style delay signal) in `carrier_score` so bufferbloat on one
carrier is visible BEFORE its delivery rate drops; (b) dampen score
oscillation with hysteresis (BALIA's coupled window moves in one
direction at a time) to avoid flapping assignments on jittery reports.

### 2.5 Per-path ACK clocking / RTT trend (MP-QUIC design lessons)

Paper: *Multipath QUIC: Design and Evaluation*, CoNEXT 2017
(Q. De Coninck, O. Bonaventure) — https://dl.acm.org/doi/10.1145/3143361.3143370
(cited by ~525). Key structural ideas: per-path packet number spaces and
ack frames (no cross-path HoL on the ACK clock), and path migration
without connection reset.

MultiTunnel already has per-carrier framing (no shared TCP), which is
why the MP-QUIC lesson mostly translates as: keep the carrier PING RTT
EWMA (S11) as the pacing signal and expose its TREND (rising fast =
bufferbloat forming) to the scheduler score — feeding 2.4(a).

### 2.6 Learning-assisted scheduling (ToN 2023) — watch item

*An Online Learning Assisted Packet Scheduler for MPTCP* (Xing et al.,
IEEE/ACM ToN 2023, USTC) learns scheduler behavior across network
scenarios for consistently quality-guaranteed mobile service
(http://staff.ustc.edu.cn/~kpxue/paper/ToN-MPTCP-YitaoXing-2023.10.pdf).
Interesting direction, but a learned policy is hard to reason about in a
safety-critical tunnel; revisit if the heuristic stack (S14 + S18)
proves insufficient on real Wi-Fi+LTE traces.

## 3. What we deliberately did NOT adopt

- **Aggressive send-window prediction (BLEST's full window math)**: our
  writer queue + peer delivery reports approximate the same signal with
  less coupling to TCP internals we do not own (Xray owns the sockets).
- **Pure redundant scheduling as the default**: N× bandwidth cost;
  Speedify themselves position Redundant Mode for calls/streams only.
- **New congestion controllers**: the carriers ride Xray TCP; CC belongs
  to the OS. We steer demand (which carrier gets which frame), not supply.
