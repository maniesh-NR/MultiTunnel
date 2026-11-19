# MultiTunnel vendored iced_tiny_skia 0.13.0

Vendored from crates.io `iced_tiny_skia 0.13.0` (terminal release of the
0.13 line) with FOUR surgical fixes — see the inline
`MultiTunnel patch (S10)` / `MultiTunnel patch (S10.1)` comments in
`src/layer.rs`, `src/lib.rs` and `src/geometry.rs`.

## Why

A canvas `Frame` records its clip rectangle in LOCAL space
(`Rectangle::new(ORIGIN, size)`), while the Canvas widget pushes an
ABSOLUTE translation (`bounds.x/y`) onto the layer stack. Upstream used
the LOCAL clip wherever ABSOLUTE window space was required:

1. `Layer::damage` (src/layer.rs): primitive bounds are mapped to
   absolute space (`bounds * transformation`) but were intersected with
   the UNTRANSFORMED local clip — every canvas not at the window origin
   produced an EMPTY intersection = ZERO damage = its fills/strokes were
   never repainted. Canvas text (a separate group that is NOT clipped in
   that closure) kept painting, which is why graphs showed labels but no
   series/grid until a coincidental full-window redraw. Cached-geometry
   damage had the mirrored bug (local bounds reported as-is).
2. The render loop (src/lib.rs): the group clip mask was built from the
   raw local clip, so on partial redraws canvas fills/strokes were masked
   to a rectangle at the window ORIGIN — content vanished or appeared
   displaced ("out of frame").

Together these are the v0.5.1 "graphs don't load correctly / appear out
of frame" report, reproduced in-sandbox (Xvfb + XTEST: connect → 12 s of
throughput → series blank, resize round-trip → series appear).

3. `Frame::fill_text` (src/geometry.rs): canvas text was pushed as
   `Text::Cached` with `(position, INFINITY, INFINITY)` marker bounds and
   an INFINITY clip. Right-alignment arithmetic downstream
   (`x -= width` → -inf) and `center()` (`-inf + inf/2` → NaN) turned
   these markers into NaN / -inf damage rectangles — the poison that made
   `iced_graphics::damage::group`'s sort abort the process (the v0.5.1
   "Check exit IP" crash; caught in the act with a temporary diagnostic
   that logged `{x:-inf, w:+inf}` and `{x:156, y:588, w:NaN}` rects).
   The fix measures the line (same no-wrap cosmic-text layout the
   glyph-outline branch uses) and stores honest bounds — anchor at the
   position, size = measured. Both consumers (`visible_bounds` for
   damage, `draw_cached` for rendering) apply the same alignment
   arithmetic to the same rectangle and now agree.

## The fixes

Map the group clip through the group transformation before intersecting
(damage) / before building the clip mask (render). Regression tests live
in `multitunnel_tests` modules at the bottom of each touched file.

4. Transform composition order (src/lib.rs, S10.1): upstream composed
   each canvas group's draw transform as
   `group.transformation() * Transformation::scale(scale_factor)`. Under
   glam's `(a * b) · v = a · (b · v)` semantics that scales FIRST and
   then adds the UNSCALED canvas offset: `physical = local·sf + offset`
   instead of the correct `(local + offset)·sf`. The two coincide only
   at `sf == 1`, so every canvas on a HiDPI display (Windows 125%/150%
   scaling — the default on laptops) was displaced
   `offset × (sf − 1)` px up-and-left of its widget, clipped by the
   correctly-computed clip mask to a sliver at the card's top edge —
   the v0.6.0 "the axes of the graph jump to the upper left, out of
   bounds" report (pixel-reproduced in the Xvfb rig at
   `WINIT_X11_SCALE_FACTOR=1.5`; fixed by composing
   `scale * group_transformation` via the `group_transform` helper in
   both the primitive and the text draw paths). Regression tests pin
   the composed mapping, its sf=1 identity, and its agreement with the
   clip mapping.

## Tracking

Pinned to upstream 0.13.0; the patch is carried via `[patch.crates-io]`
in the root `Cargo.toml`. When iced ships a fixed 0.13.x/0.14, re-point
the dependency and delete this directory.

License: MIT (upstream iced), unchanged.
