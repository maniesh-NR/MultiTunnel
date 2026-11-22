# S20 Copilot code review transcript (v0.13.1, PR #1)

Reviewer: copilot-pull-request-reviewer[bot] via the REST endpoint
POST /repos/maniesh-NR/MultiTunnel/pulls/1/copilot-code-review.

Effort ladder (per the fallback instruction): `max` → rejected by the
account's remaining quota; `balanced` → rejected; `lite` → ran.
The standalone @github/copilot CLI path stayed blocked: the fine-grained
PAT lacks the "Copilot Requests" permission
(api.individual.githubcopilot.com 401 — authentic server response; the
permission can only be added by editing the PAT in the GitHub web UI).


---

## Round 1 — effort Lite, commit 52f1d3c

### Review summary

```
<!-- ccr-overview-v2 -->

### 🟡 Changes recommended

Multiple unresolved critical correctness and security issues remain, including kill-switch races and failed restoration handling that can leave traffic misrouted, plus diagnostic URL leakage of credentials.

<details open>
<summary><strong>12 open issues</strong></summary>

- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.png" alt="High severity" width="62" height="18" align="texttop"></picture> [Clear proxy ownership after kill-switch self-heal](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411193) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.png" alt="High severity" width="62" height="18" align="texttop"></picture> [Serialize kill-switch proxy changes with disconnect](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411210) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.png" alt="High severity" width="62" height="18" align="texttop"></picture> [Bind the kill-switch listener before setting the system proxy](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411223) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.png" alt="High severity" width="62" height="18" align="texttop"></picture> [Keep the kill-switch listener until proxy restore succeeds](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411231) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/high-v2-light.png" alt="High severity" width="62" height="18" align="texttop"></picture> [Redact secrets from user-configurable diagnostic URLs](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411240) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.png" alt="Medium severity" width="62" height="18" align="texttop"></picture> [Apply debug logging when diagnostics is enabled live](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411265) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.png" alt="Medium severity" width="62" height="18" align="texttop"></picture> [Recover the diagnostics toggle when sink startup fails](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411279) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.png" alt="Medium severity" width="62" height="18" align="texttop"></picture> [Match the live session profile before probing its exit IP](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411287) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.png" alt="Medium severity" width="62" height="18" align="texttop"></picture> [Cap incomplete HTTP response headers](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411295) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.png" alt="Medium severity" width="62" height="18" align="texttop"></picture> [Reject server port zero when editing profiles](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411304) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.png" alt="Medium severity" width="62" height="18" align="texttop"></picture> [Validate edited REALITY fields before saving](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411318) · New
- <picture><source media="(prefers-color-scheme: dark)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-dark.svg"><source media="(prefers-color-scheme: light)" srcset="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.svg"><img src="https://github.githubassets.com/static/images/icons/copilot-code-review/medium-v2-light.png" alt="Medium severity" width="62" height="18" align="texttop"></picture> [Select the top-priority adapter for offline tests](https://github.com/maniesh-NR/MultiTunnel/pull/1#discussion_r4656411330) · New
</details>

<sub>Review effort: Lite</sub>
```


### Findings and discussion


#### [4656411193] src/real_service.rs L922

**Copilot:** The self-heal restores the OS proxy and stops the black-hole, but it never clears `core.sysproxy_set`. After a kill-switch failure, the next connect with `system_proxy` disabled leaves this flag set; a later disconnect then calls `restore()` with no snapshot and can disable the user's pre-existing proxy instead of leaving it untouched. Reset the ownership flag together with this self-heal.


**Maintainer:** **Fixed in 4923566.** The step-0 self-heal now clears `sysproxy_set` on a successful restore (and — new this round — keeps the flag AND the listener when the restore fails, so the next disconnect retries instead of stranding a black-holed proxy). Regression test: `connect_selfheal_clears_stale_proxy_ownership`.


#### [4656411210] src/real_service.rs L1363

**Copilot:** This check-and-set is not serialized with `disconnect()`: after `give_up` loads `kill_switch = true`, an explicit disconnect can swap `sysproxy_set` to false and restore the snapshot, then this awaited `sysproxy::set` can still install the black-hole and leave `sysproxy_set` true after disconnect has returned. That can re-block traffic even though the user explicitly disconnected. Serialize proxy hand-off with disconnect, or re-check ownership/settings after every awaited step and roll back if the session was released.


**Maintainer:** **Fixed in 4923566.** New `Core::proxy_mtx` (std Mutex) is held across every proxy hand-off: `give_up`'s engage/restore decision, both disconnect branches, the kill-switch toggle-off escape hatch, the step-0 self-heal and the connect-time arming. `give_up` re-reads the flag under the lock, so the disconnect-then-re-block interleaving is gone. The black-hole bind is hoisted OUT of the lock (it awaits), so no std lock is ever held across an await — the crate's standing rule.


#### [4656411223] src/real_service.rs L1367

**Copilot:** The proxy is pointed at `127.0.0.1:9` before `spawn_blackhole` binds that port. A local process can therefore bind the port in this window (or already own it); `sysproxy::set` succeeds, the subsequent bind fails, and WinINET traffic is sent to that process rather than a fail-closed listener. Bind and retain the listener before changing the system proxy, and only engage the kill switch if ownership succeeds (or otherwise use a per-run listener address that is reserved first).


**Maintainer:** **Fixed in 4923566.** The order is now bind → set: `bind_blackhole()` runs before any registry write, and `sysproxy::set` only executes when ownership is proven. On a bind failure the stale entry (which refuses) keeps blocking and nothing is ever routed to a foreign listener; on a `set` failure after a successful bind the freshly-bound listener is released. `spawn_blackhole` was split into `bind_blackhole` + `engage_blackhole` to make the locked section fully synchronous.


#### [4656411231] src/real_service.rs L1620

**Copilot:** The no-session disconnect path clears `sysproxy_set` before restoring, then stops the black-hole listener even when `restore` returns an error. In that failure case the system proxy can still point at `127.0.0.1:9`, but the listener is gone and the ownership flag is false, so later disconnects will not retry and the unconditional "system proxy restored" log is false. Keep the kill-switch ownership/listener until restore succeeds and surface a failed restore instead of claiming success.

This issue also appears in the following locations of the same file:
- line 1635
- line 1860


**Maintainer:** **Fixed in 4923566 (all three sites).** Every restore path now matches on the result: `Ok` clears the flag, stops the listener and logs “restored”; `Err` keeps the flag AND the listener (still fail-closed), emits an `ERROR system proxy restore failed … retry` GUI log line, and the same path retries on the next disconnect/toggle. The “system proxy restored” line only appears after a successful restore. Note: the failure branch is Windows-registry-only, so it is covered by review rather than a Linux unit test — the retry semantics themselves are what the tests pin.


#### [4656411240] src/real_service.rs L2438

**Copilot:** These user-configurable URLs are written verbatim to the supposedly redacted support file. A URL may contain a query token, API key, or other secret (for example a custom probe endpoint), so enabling diagnostics can disclose credentials even though profile UUIDs and public keys are omitted. Redact URL userinfo/query/fragment or log only the validated host, port, and path components.


**Maintainer:** **Fixed in 4923566.** New `redact_url()`: the header renders `scheme://host:port/path` only — userinfo (`user:pass@`), query (`?token=…`) and fragment are stripped, and an unparseable URL renders as `<redacted (invalid URL)>` without ever echoing the raw value. Regression test: `diag_header_redacts_url_credentials`.


#### [4656411265] src/real_service.rs L1986

**Copilot:** Turning diagnostics on while already connected does not update the running engines' log level: `debug` is only applied in `connect_flow()` before a new session is built, while this settings path enables the sink without reconnecting when no data-plane setting changed. The resulting file misses the engine debug output the UI promises. Reconnect the live session or provide an engine log-level update when diagnostics is enabled.


**Maintainer:** **Fixed in 4923566.** Enabling diagnostics while Connected now reconnects the session (the engines read the log level only at spawn, so a live update cannot exist without an engine restart mechanism); the reconnect log line says why. Regression test: `diagnostics_enable_while_connected_reconnects` (state back to Connected, restarts counter incremented, engine level `debug`).


#### [4656411279] src/real_service.rs L2011

**Copilot:** If `diagnostics::enable` fails (for example, the log directory is unwritable), the settings file has already been saved with `diagnostics = true`, but the sink remains off. The UI consequently shows the mode enabled while no file is being written, and a later settings refresh will not retry because `diagnostics_changed` is false. Revert the persisted toggle on failure or surface a state that can be retried explicitly.


**Maintainer:** **Fixed in 4923566.** A failed `diagnostics::enable` reverts the persisted toggle (the settings file is saved back with `diagnostics = false`), emits an `ERROR … the toggle was reverted` GUI line, and the live reconnect is suppressed for the failed enable. The retry path is simply toggling again — the diff is real again because the file says off. Regression test: `diagnostics_enable_failure_reverts_the_toggle` (log dir replaced by a file).


#### [4656411287] src/real_service.rs L1954

**Copilot:** The same stale-session problem affects the exit probe: this branch treats any existing session as the active profile's live tunnel, but the active profile can be changed without reconnecting. In that state the probe reports the old tunnel's exit IP against the new profile's endpoint and labels it as a live-tunnel result. The session needs to retain its profile id and this path must use the live listener only when it matches the selected profile; otherwise use the temporary tunnel.

This issue also appears on line 2050 of the same file.


**Maintainer:** **Fixed in 4923566.** `Session` now records the `profile_id` it was built from. `exit_probe` uses the live listener only when the session belongs to the active profile, and `speed_test` (which had the same hole via its `is_active` check — it compared the TARGET against the active profile, not against the session) now matches the session against the target profile directly. Regression test: `probes_fall_back_when_active_profile_switched` (live probe → active switched without reconnect → both probes report the temporary tunnel; an explicit target matching the session still rides it).


#### [4656411295] src/real_service.rs L2335

**Copilot:** The 64 MiB limit only applies after the response head has been found. Until `find_head_end` succeeds, every read is appended to `head` while `total` stays zero, so an endpoint that continuously sends bytes without `\r\n\r\n` can grow this `Vec` for the whole 30-second window and bypass the advertised memory cap. Cap the header buffer (or fail on an oversized/incomplete head) independently of the payload limit.


**Maintainer:** **Fixed in 4923566.** `SPEED_TEST_MAX_HEAD` (64 KiB) is enforced independently while the head is still incomplete — the read loop now fails fast with “malformed response (no header end within 64 KiB)” instead of buffering for the whole budget. Regression test: `speed_test_rejects_a_header_flood` (256 KiB header-less flood, error in well under 10 s).


#### [4656411304] src/real_service.rs L2197

**Copilot:** `u16::parse()` accepts `0`, so an edit can save a server port of zero even though imported profiles and the edit form require a real server port (`1-65535`). `endpoint_from_profile()` does not reject this value, so the malformed profile persists and only fails later during engine connection. Reject zero here (and align the error text with the actual range).


**Maintainer:** **Fixed in 4923566.** Port 0 is rejected explicitly (“port 0 is not a valid server port — use 1-65535”) and the parse-error text now says 1-65535. Regression test: `update_profile_rejects_port_zero_and_invalid_reality`.


#### [4656411318] src/real_service.rs L2435

**Copilot:** The edit path only checks that the original profile remains convertible; it does not validate the edited REALITY material. This allows saving an empty SNI, a non-43-character/non-base64url public key, or an invalid short ID, all of which the link parser/render validator rejects and the Xray config then receives unchanged. Validate these fields with the same structural rules used for imported links before `store.update()`.


**Maintainer:** **Fixed in 4923566.** Inside the existing capability gate (an original that could drive the engine must stay convertible), the edited REALITY material is validated with the link parser's exact structural rules: SNI non-empty, `pbk` 43-char unpadded base64url (`crate::vless::is_base64url`, now crate-visible), `sid` empty-or-1-16-hex. Deliberate non-change: `fingerprint` is NOT validated — the import parser accepts any `fp=` verbatim and the edit path keeps parity with imports. Regression test: same test as the port-0 fix (plus a valid edit still saving).


#### [4656411330] src/real_service.rs L2342

**Copilot:** `engine_specs_for()` and `EngineLinkFactory::build()` preserve the user's bonded-adapter order, but `pop()` selects the last link, contrary to this function's documented “FIRST link (top-priority adapter)” behavior. Offline exit-IP and speed tests will therefore silently run on the lowest-priority adapter whenever bonding is configured. Select the first link and drop the remainder instead.


**Maintainer:** **Fixed in 4923566.** `temp_link_for` takes the FIRST link (`into_iter().next()`); `test_latency`'s identical `pop()` was fixed too (same defect class, pre-existing this PR — flagged as an extension during triage). Regression test: `temp_link_uses_the_top_priority_first_link` (two named links to two different exit echoes; the probe answers from the FIRST).


---

## Round 2 — effort Lite, commit 4923566 (the fixes)

Review 6010239912 on 2026-11-22T04:07:25Z: state COMMENTED, empty body,
ZERO inline comments — a clean pass. No new findings.


---

## Outcome

All 12 round-1 findings fixed in 4923566 (v0.13.1); round-2 re-review
clean. PR #1 closed with the summary comment. Tests 474/0 (+8).
