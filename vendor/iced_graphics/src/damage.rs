//! Compute the damage between frames.
use crate::core::{Point, Rectangle};

/// Diffs the damage regions given some previous and current primitives.
pub fn diff<T>(
    previous: &[T],
    current: &[T],
    bounds: impl Fn(&T) -> Vec<Rectangle>,
    diff: impl Fn(&T, &T) -> Vec<Rectangle>,
) -> Vec<Rectangle> {
    let damage = previous.iter().zip(current).flat_map(|(a, b)| diff(a, b));

    if previous.len() == current.len() {
        damage.collect()
    } else {
        let (smaller, bigger) = if previous.len() < current.len() {
            (previous, current)
        } else {
            (current, previous)
        };

        // Extend damage by the added/removed primitives
        damage
            .chain(bigger[smaller.len()..].iter().flat_map(bounds))
            .collect()
    }
}

/// Computes the damage regions given some previous and current primitives.
pub fn list<T>(
    previous: &[T],
    current: &[T],
    bounds: impl Fn(&T) -> Vec<Rectangle>,
    are_equal: impl Fn(&T, &T) -> bool,
) -> Vec<Rectangle> {
    diff(previous, current, &bounds, |a, b| {
        if are_equal(a, b) {
            vec![]
        } else {
            bounds(a).into_iter().chain(bounds(b)).collect()
        }
    })
}

/// Groups the given damage regions that are close together inside the given
/// bounds.
pub fn group(mut damage: Vec<Rectangle>, bounds: Rectangle) -> Vec<Rectangle> {
    const AREA_THRESHOLD: f32 = 20_000.0;

    // MultiTunnel patch (S10): the upstream comparator used
    // `partial_cmp(..).unwrap_or(Ordering::Equal)`, which silently returns
    // `Equal` whenever either side is NaN. `Rectangle::INFINITE.center()`
    // is `-inf + inf/2 = NaN`, so any infinite damage rectangle (layer
    // bounds of the unclipped base layer, unclipped item bounds, …) made
    // the comparator inconsistent — `NaN == a`, `NaN == b`, yet `a < b` —
    // and Rust's driftsort aborts the whole process with
    // "user-provided comparison function does not correctly implement a
    // total order" (reproducible: connect → click "Check exit IP").
    // `f32::total_cmp` is a genuine total order (IEEE 754 totalOrder);
    // NaN rectangles still drop out harmlessly below, because
    // `Rectangle::intersection` with NaN coordinates yields `None`.
    damage.sort_by(|a, b| {
        a.center()
            .distance(Point::ORIGIN)
            .total_cmp(&b.center().distance(Point::ORIGIN))
    });

    let mut output = Vec::new();
    let mut scaled = damage
        .into_iter()
        .filter_map(|region| region.intersection(&bounds))
        .filter(|region| region.width >= 1.0 && region.height >= 1.0);

    if let Some(mut current) = scaled.next() {
        for region in scaled {
            let union = current.union(&region);

            if union.area() - current.area() - region.area() <= AREA_THRESHOLD {
                current = union;
            } else {
                output.push(current);
                current = region;
            }
        }

        output.push(current);
    }

    output
}

#[cfg(test)]
mod multitunnel_tests {
    use super::*;
    use crate::core::{Point, Rectangle, Size};

    /// MultiTunnel S10 regression: the shapes that poisoned the v0.5.1
    /// damage sort. A right-aligned canvas label produced
    /// `{x: -inf, width: +inf}` (its `x -= width` alignment arithmetic),
    /// whose center is NaN — the upstream comparator answered `Equal` for
    /// NaN-vs-anything, driftsort detected the broken total order and
    /// aborted the process (the "Check exit IP" crash). With `total_cmp`
    /// the sort must complete; NaN rectangles drop out via `intersection`
    /// (NaN comparisons are false → `None`), and `Rectangle::INFINITE`
    /// (origin-based, center = inf, which is CONSISTENT) still clamps to
    /// the viewport = a full repaint — the safe answer.
    #[test]
    fn group_survives_nan_and_infinite_rectangles() {
        let viewport = Rectangle::new(Point::ORIGIN, Size::new(1280.0, 800.0));

        // The REAL poisoner shape: right-aligned label marker bounds.
        let right_aligned_nan = Rectangle {
            x: f32::NEG_INFINITY,
            y: 588.0,
            width: f32::INFINITY,
            height: 20.0,
        };
        assert!(right_aligned_nan.center_x().is_nan());

        let mut damage = vec![
            Rectangle::INFINITE,
            Rectangle::new(Point::new(10.0, 10.0), Size::new(50.0, 50.0)),
            right_aligned_nan,
            Rectangle::new(Point::new(600.0, 400.0), Size::new(80.0, 60.0)),
            Rectangle {
                x: f32::NAN,
                y: f32::NAN,
                width: f32::NAN,
                height: f32::NAN,
            },
            Rectangle::new(Point::new(900.0, 100.0), Size::new(30.0, 30.0)),
        ];
        // Shuffle so the sort really reorders (a sorted input may not
        // exercise the inconsistent-comparator path).
        damage.reverse();

        let grouped = group(damage, viewport);

        // INFINITE clamps to the viewport; NaN rectangles vanish; the
        // finite rectangles survive (possibly merged with the viewport).
        assert!(
            grouped.iter().any(|r| *r == viewport),
            "INFINITE must clamp to a full-viewport repaint: {grouped:?}"
        );
        assert!(
            grouped.iter().all(|r| r.x.is_finite() && r.y.is_finite()),
            "NaN rectangles must be dropped: {grouped:?}"
        );
    }
}
