# v0.2.0-AUDIT-B (build/portability/license, on the recycled Nov-12 tree) — agent-0faf5e78
APPROVE. 0 P0 / 2 P1 / 5 P2. Key P1s: (1) restart-campaign loop only tested
in its degenerate max=0 form — CampaignOutcome::Restarted and the max>0
exhaustion path have zero coverage; (2) S2 process/lifecycle semantics have
zero runtime coverage on Windows (rig is unix-only) and the rig sets
system_proxy:true — porting it naively to Windows CI would mutate the real
HKCU registry of the runner.
Verified: 52/52 host tests (per-module map: config 10, core_proc 5, elevate
3, netmerge 14, supervisor 8, sysproxy 6, xrayconf 6), host build green,
zigbuild green, `cargo check --target x86_64-pc-windows-msvc` green (full
windows cfg tree incl. winreg + windows-sys; raw-dylib linkage eliminates
import-lib divergence); single unified windows-sys 0.61.2 in the graph;
winreg 0.56 is MIT-only (S4 notices must record that); 6/7 windows-sys
features used (LibraryLoader unused); no secrets in tracked files; both
workflows valid; file(1): console + GUI subsystems correct.
