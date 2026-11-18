//! Real-time bandwidth graph (D19): an iced canvas Program plotting the
//! last `N` one-second throughput samples as two filled area series
//! (down = primary, up = tertiary), Task-Manager style. The canvas is
//! `Length::Fill`-sized, so it scales with the window.

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
}

impl RateGraph {
    /// Build the canvas widget at fill size.
    pub fn canvas(self) -> Canvas<Self, Message, iced::Theme, iced::Renderer> {
        Canvas::new(self).width(Length::Fill).height(Length::Fill)
    }
}

/// Map `(i, rate)` to a point inside `bounds`; the series is right-aligned
/// (newest sample at the right edge) over the full sample window.
/// (`Rectangle` is f32 — convert at the edges.)
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
            let y = bounds.y + bounds.height * (k as f32 + 1.0) / 5.0;
            let line = Path::line(
                Point::new(bounds.x, y),
                Point::new(bounds.x + bounds.width, y),
            );
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
        let b = bounds;
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

        // Labels: ceiling top-left, current rates top-right, window bottom.
        frame.fill_text(Text {
            content: format_rate(ceiling),
            position: Point::new(bounds.x + 4.0, bounds.y + 2.0),
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
                position: Point::new(bounds.x + bounds.width - 4.0, bounds.y + 2.0),
                color: st.label,
                size: iced::Pixels(self.font_size),
                horizontal_alignment: iced::alignment::Horizontal::Right,
                ..Default::default()
            });
        }
        frame.fill_text(Text {
            content: format!("last {n} s"),
            position: Point::new(
                bounds.x + 4.0,
                bounds.y + bounds.height - self.font_size - 2.0,
            ),
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

    #[test]
    fn rates_format_human() {
        assert_eq!(format_rate(240.0), "240 B/s");
        assert_eq!(format_rate(1024.0 * 1.5), "1.5 KB/s");
        assert_eq!(format_rate(1024.0 * 1024.0 * 2.4), "2.4 MB/s");
    }
}
