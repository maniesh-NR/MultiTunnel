//! Real-time bandwidth graph (D19, fixed in D20): an iced canvas Program
//! plotting the last `N` one-second throughput samples as two filled area
//! series (down = primary, up = tertiary), Task-Manager style, with an
//! in-canvas color legend. The canvas is `Length::Fill`-sized, so it scales
//! with the window.
//!
//! COORDINATE CONTRACT (the D20 fix): iced draws a canvas through
//! `renderer.with_translation(bounds.x, bounds.y)` — a `Program::draw` frame
//! is in LOCAL space where (0,0) is the canvas' own top-left. The v0.5.0
//! graph computed every point from `bounds.x`/`bounds.y`, double-offsetting
//! the whole plot out of the visible card (the "graph does not load" bug).
//! All helpers here now take the LOCAL rectangle (`Rectangle::new(ORIGIN,
//! bounds.size())`) and the regression test pins that invariant.

use iced::widget::canvas::{self, Canvas, Frame, Path, Text};
use iced::{Color, Length, Point, Rectangle};

use crate::Message;

/// One throughput sample: (up_bps, down_bps) in bytes per second.
pub type Sample = (u64, u64);

/// Theme inputs (pre-resolved from the M3 scheme so the Program is Copy
/// and self-contained per render).
#[derive(Debug, Clone, Copy)]
pub struct RateGraphStyle {
    pub down_stroke: Color,
    pub down_fill: Color,
    pub up_stroke: Color,
    pub up_fill: Color,
    pub grid: Color,
    pub label: Color,
}

/// The plotted graph: samples are PRE-CLONED into the program per render
/// (the App pushes one sample per Throughput event; the view rebuilds the
/// program, which invalidates the canvas).
#[derive(Debug, Clone)]
pub struct RateGraph {
    pub samples: Vec<Sample>,
    pub style: RateGraphStyle,
    /// Small text size for axis labels.
    pub font_size: f32,
    /// Show the color legend (combined graph; off for per-adapter minis —
    /// the colors are identical across graphs, one legend is enough).
    pub legend: bool,
}

impl RateGraph {
    /// Build the canvas widget at fill size (the Status screen's combined
    /// graph — scales with the window).
    pub fn canvas(self) -> Canvas<Self, Message, iced::Theme, iced::Renderer> {
        Canvas::new(self).width(Length::Fill).height(Length::Fill)
    }

    /// Build a fixed-height mini canvas (per-adapter graph rows).
    pub fn mini(self, height: f32) -> Canvas<Self, Message, iced::Theme, iced::Renderer> {
        Canvas::new(self)
            .width(Length::Fill)
            .height(Length::Fixed(height))
    }
}

/// Map `(i, rate)` to a point inside `bounds`; the series is right-aligned
/// (newest sample at the right edge) over the full sample window.
/// `bounds` is the LOCAL canvas rectangle (see the module doc).
fn point(bounds: Rectangle, i: usize, n: usize, rate: f64, max: f64) -> Point {
    let n_slots = n.max(2) as f64;
    // Right edge = newest; spread over the full width.
    let x = if n <= 1 {
        f64::from(bounds.x + bounds.width)
    } else {
        f64::from(bounds.x) + (i as f64 / (n_slots - 1.0)) * f64::from(bounds.width)
    };
    // 12 px head-room at the top so the stroke never clips.
    let usable = (f64::from(bounds.height) - 12.0).max(1.0);
    let y = f64::from(bounds.y) + usable - (rate / max).clamp(0.0, 1.0) * usable;
    Point::new(x as f32, y as f32)
}

/// Nice ceiling for the y axis: 1/2/5 × 10^k covering `max`.
pub fn nice_ceiling(max: f64) -> f64 {
    if max <= 0.0 {
        return 1.0;
    }
    let exp = max.log10().floor() as i32;
    let base = 10f64.powi(exp);
    for m in [1.0, 2.0, 5.0, 10.0] {
        let v = m * base;
        if v >= max {
            return v;
        }
    }
    max
}

/// The local rectangle the program draws into (D20: everything below uses
/// THIS, never the widget-space bounds origin).
fn local_bounds(bounds: Rectangle) -> Rectangle {
    Rectangle::new(Point::ORIGIN, bounds.size())
}

impl canvas::Program<Message> for RateGraph {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let b = local_bounds(bounds);
        let st = self.style;
        let n = self.samples.len();

        // Axis ceiling from the visible max of both series.
        let max_rate = self
            .samples
            .iter()
            .map(|(u, d)| (*u).max(*d) as f64)
            .fold(0.0_f64, f64::max);
        let ceiling = nice_ceiling(max_rate);

        // Grid: 4 horizontal lines.
        for k in 0..4 {
            let y = b.y + b.height * (k as f32 + 1.0) / 5.0;
            let line = Path::line(Point::new(b.x, y), Point::new(b.x + b.width, y));
            frame.stroke(
                &line,
                canvas::Stroke::default()
                    .with_color(st.grid)
                    .with_width(1.0),
            );
        }

        // Series drawing: filled area + stroke (iced 0.13 paths are built
        // through a closure builder).
        fn series_path<F>(
            bounds: Rectangle,
            n: usize,
            ceiling: f64,
            pick: F,
            close_bottom: bool,
        ) -> Path
        where
            F: Fn(usize) -> f64,
        {
            Path::new(move |b| {
                b.move_to(point(bounds, 0, n, pick(0), ceiling));
                for i in 1..n {
                    b.line_to(point(bounds, i, n, pick(i), ceiling));
                }
                if close_bottom {
                    b.line_to(Point::new(
                        bounds.x + bounds.width,
                        bounds.y + bounds.height,
                    ));
                    b.line_to(Point::new(bounds.x, bounds.y + bounds.height));
                    b.close();
                }
            })
        }
        fn draw_series<F>(
            frame: &mut Frame,
            bounds: Rectangle,
            n: usize,
            ceiling: f64,
            pick: F,
            stroke: Color,
            fill: Color,
        ) where
            F: Fn(usize) -> f64,
        {
            if n == 0 {
                return;
            }
            frame.fill(&series_path(bounds, n, ceiling, &pick, true), fill);
            frame.stroke(
                &series_path(bounds, n, ceiling, pick, false),
                canvas::Stroke::default().with_color(stroke).with_width(1.5),
            );
        }
        let c = ceiling;
        draw_series(
            &mut frame,
            b,
            n,
            c,
            |i| self.samples[i].1 as f64,
            st.down_stroke,
            st.down_fill,
        );
        draw_series(
            &mut frame,
            b,
            n,
            c,
            |i| self.samples[i].0 as f64,
            st.up_stroke,
            st.up_fill,
        );

        // Legend (D20): a short color sample + label per series, top-left,
        // under the ceiling label. One glance answers "which line is which".
        if self.legend {
            let fs = self.font_size;
            let sample_w = 10.0;
            let swatch_h = (fs * 0.55).max(3.0);
            let y = b.y + 2.0 + fs + 6.0;
            let mut x = b.x + 4.0;
            for (color, label) in [(st.down_stroke, "down"), (st.up_stroke, "up")] {
                frame.fill_rectangle(
                    Point::new(x, y + (fs - swatch_h) / 2.0),
                    iced::Size::new(sample_w, swatch_h),
                    color,
                );
                frame.fill_text(Text {
                    content: label.to_string(),
                    position: Point::new(x + sample_w + 4.0, y),
                    color: st.label,
                    size: iced::Pixels(fs),
                    ..Default::default()
                });
                // Advance: swatch + gap + text (~3.5 chars) + inter-entry gap.
                x += sample_w + 4.0 + fs * 3.5 + 12.0;
            }
        }

        // Labels: ceiling top-left, current rates top-right, window bottom.
        frame.fill_text(Text {
            content: format_rate(ceiling),
            position: Point::new(b.x + 4.0, b.y + 2.0),
            color: st.label,
            size: iced::Pixels(self.font_size),
            ..Default::default()
        });
        if let Some((u, d)) = self.samples.last() {
            frame.fill_text(Text {
                content: format!(
                    "↓ {}   ↑ {}",
                    format_rate(*d as f64),
                    format_rate(*u as f64)
                ),
                position: Point::new(b.x + b.width - 4.0, b.y + 2.0),
                color: st.label,
                size: iced::Pixels(self.font_size),
                horizontal_alignment: iced::alignment::Horizontal::Right,
                ..Default::default()
            });
        }
        frame.fill_text(Text {
            content: format!("last {n} s"),
            position: Point::new(b.x + 4.0, b.y + b.height - self.font_size - 2.0),
            color: st.label,
            size: iced::Pixels(self.font_size),
            ..Default::default()
        });

        vec![frame.into_geometry()]
    }
}

/// Human rate formatting ("240 B/s", "1.2 MB/s") matching the app's chips.
pub fn format_rate(bps: f64) -> String {
    const UNIT: f64 = 1024.0;
    if bps < UNIT {
        format!("{bps:.0} B/s")
    } else if bps < UNIT * UNIT {
        format!("{:.1} KB/s", bps / UNIT)
    } else if bps < UNIT * UNIT * UNIT {
        format!("{:.1} MB/s", bps / (UNIT * UNIT))
    } else {
        format!("{:.1} GB/s", bps / (UNIT * UNIT * UNIT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ceiling_covers_and_rounds() {
        assert_eq!(nice_ceiling(0.0), 1.0);
        assert_eq!(nice_ceiling(0.4), 0.5);
        assert_eq!(nice_ceiling(0.6), 1.0);
        assert_eq!(nice_ceiling(1.0), 1.0);
        assert_eq!(nice_ceiling(1.5), 2.0);
        assert_eq!(nice_ceiling(3.0), 5.0);
        assert_eq!(nice_ceiling(6.0), 10.0);
        assert_eq!(nice_ceiling(999_000.0), 1_000_000.0);
        assert_eq!(nice_ceiling(1_500_000.0), 2_000_000.0);
    }

    #[test]
    fn points_stay_inside_bounds_and_right_align() {
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 100.0,
        };
        let n = 4;
        // Empty-rate series sits on the bottom; full-rate near the top.
        let bottom = point(bounds, 0, n, 0.0, 100.0);
        assert_eq!(bottom.y, 100.0 - 12.0);
        let top = point(bounds, 3, n, 100.0, 100.0);
        assert!((top.y - 0.0).abs() < 1e-6, "full rate hits the top");
        assert!((top.x - 300.0).abs() < 1e-6, "newest sample at right edge");
        let first = point(bounds, 0, n, 50.0, 100.0);
        assert!((first.x - 0.0).abs() < 1e-6, "oldest sample at left edge");
        assert!((first.y - (100.0 - 12.0) / 2.0).abs() < 1e-6);
        // Single-sample series must not divide by zero.
        let single = point(bounds, 0, 1, 10.0, 10.0);
        assert!((single.x - 300.0).abs() < 1e-6);
    }

    /// D20 regression: the v0.5.0 graph drew in WIDGET space (points from
    /// `bounds.x`/`bounds.y`) while iced translates the frame by the bounds
    /// origin — everything landed outside the card ("graph does not load").
    /// `local_bounds` must strip the origin; every plotted point must stay
    /// inside [0, width] × [0, height] for ANY widget placement.
    #[test]
    fn local_bounds_strips_widget_offset_regression() {
        // Where iced actually places the status graph (right of the nav
        // rail, below the state/exit cards).
        let widget_bounds = Rectangle {
            x: 170.0,
            y: 190.0,
            width: 300.0,
            height: 100.0,
        };
        let b = local_bounds(widget_bounds);
        assert_eq!(b.x, 0.0);
        assert_eq!(b.y, 0.0);
        assert_eq!(b.width, 300.0);
        assert_eq!(b.height, 100.0);

        let n = 4;
        for i in 0..n {
            for rate in [0.0, 50.0, 100.0] {
                let p = point(b, i, n, rate, 100.0);
                assert!(
                    (0.0..=300.0).contains(&f64::from(p.x)),
                    "x {} outside the local canvas",
                    p.x
                );
                assert!(
                    (0.0..=100.0).contains(&f64::from(p.y)),
                    "y {} outside the local canvas",
                    p.y
                );
            }
        }
    }

    #[test]
    fn rates_format_human() {
        assert_eq!(format_rate(240.0), "240 B/s");
        assert_eq!(format_rate(1024.0 * 1.5), "1.5 KB/s");
        assert_eq!(format_rate(1024.0 * 1024.0 * 2.4), "2.4 MB/s");
    }
}
