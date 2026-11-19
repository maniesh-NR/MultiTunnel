# MultiTunnel vendored iced_graphics 0.13.0

Vendored from crates.io `iced_graphics 0.13.0` (terminal release of the
0.13 line) with ONE surgical fix — see `git diff` against the pristine
copy for the exact change and its inline `MultiTunnel patch (S10)` comment.

## Why

`damage::group()` sorted damage rectangles with
`partial_cmp(..).unwrap_or(Ordering::Equal)`. `Rectangle::INFINITE.center()`
is NaN (`-inf + inf/2`), and NaN-vs-anything answered `Equal`, making the
comparator inconsistent (not a total order). Rust's driftsort detects this
and panics:

    panicked at library/core/src/slice/sort/shared/smallsort.rs:854:5:
    user-provided comparison function does not correctly implement a total order

Reproduced deterministically in-sandbox (Xvfb + XTEST input): connect →
click "Check exit IP" → exit card text change grows the damage list →
sort hits the inconsistent triple → process aborts. This was the v0.5.1
"check exit ip crashes the app" report.

## The fix

`f32::total_cmp` (IEEE 754 totalOrder) — a genuine total order. NaN
rectangles are still dropped afterwards by `Rectangle::intersection`
(NaN comparisons are false → `None`); INFINITE rectangles clamp to the
viewport = a full repaint, which is always safe.

## Tracking

Pinned to upstream 0.13.0; the patch is carried via `[patch.crates-io]`
in the root `Cargo.toml`. When iced ships a fixed 0.13.x/0.14, re-point
the dependency and delete this directory.

License: MIT (upstream iced), unchanged.
