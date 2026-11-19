#![allow(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
pub mod window;

mod engine;
mod layer;
mod primitive;
mod settings;
mod text;

#[cfg(feature = "image")]
mod raster;

#[cfg(feature = "svg")]
mod vector;

#[cfg(feature = "geometry")]
pub mod geometry;

pub use iced_graphics as graphics;
pub use iced_graphics::core;

pub use layer::Layer;
pub use primitive::Primitive;
pub use settings::Settings;

#[cfg(feature = "geometry")]
pub use geometry::Geometry;

use crate::core::renderer;
use crate::core::{
    Background, Color, Font, Pixels, Point, Rectangle, Transformation,
};
use crate::engine::Engine;
use crate::graphics::compositor;
use crate::graphics::text::{Editor, Paragraph};
use crate::graphics::Viewport;

/// Composes a canvas group's LOCAL transformation (logical-space offsets,
/// e.g. the canvas widget's `with_translation(bounds.x, bounds.y)`) with
/// the viewport scale factor, producing the transformation that maps
/// LOCAL canvas coordinates to PHYSICAL pixels.
///
/// MultiTunnel patch (S10.1): upstream multiplied in the OTHER order —
/// `group_transformation * scale` — which, under glam's `(a * b) · v =
/// a · (b · v)` semantics, scales FIRST and then adds the UNSCALED
/// offset: `physical = local·sf + offset`. The correct mapping is
/// `physical = (local + offset)·sf = local·sf + offset·sf`. The two
/// only coincide at `sf == 1`, so the bug was invisible at 100% DPI:
/// on any HiDPI display (Windows 125%/150% scaling is the default on
/// laptops) every canvas' content — the MultiTunnel bandwidth graph's
/// grid, series and labels — landed `offset·(sf − 1)` px up and to the
/// LEFT of its card, clipped by the (correctly computed) clip mask to
/// a sliver at the card's top edge (the v0.6.0 "the axes of the graph
/// jump to the upper left, out of bounds" report; pixel-reproduced in
/// the Xvfb rig at `WINIT_X11_SCALE_FACTOR=1.5`). Note the clip mapping
/// in `Renderer::draw` chains `rect * group * scale` left-to-right,
/// which applies translate first and scale second — the correct order —
/// so only these two composition sites needed the swap.
fn group_transform(
    group_transformation: Transformation,
    scale_factor: f32,
) -> Transformation {
    Transformation::scale(scale_factor) * group_transformation
}

/// A [`tiny-skia`] graphics renderer for [`iced`].
///
/// [`tiny-skia`]: https://github.com/RazrFalcon/tiny-skia
/// [`iced`]: https://github.com/iced-rs/iced
#[derive(Debug)]
pub struct Renderer {
    default_font: Font,
    default_text_size: Pixels,
    layers: layer::Stack,
    engine: Engine, // TODO: Shared engine
}

impl Renderer {
    pub fn new(default_font: Font, default_text_size: Pixels) -> Self {
        Self {
            default_font,
            default_text_size,
            layers: layer::Stack::new(),
            engine: Engine::new(),
        }
    }

    pub fn layers(&mut self) -> &[Layer] {
        self.layers.flush();
        self.layers.as_slice()
    }

    pub fn draw<T: AsRef<str>>(
        &mut self,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        viewport: &Viewport,
        damage: &[Rectangle],
        background_color: Color,
        overlay: &[T],
    ) {
        let physical_size = viewport.physical_size();
        let scale_factor = viewport.scale_factor() as f32;

        if !overlay.is_empty() {
            let path = tiny_skia::PathBuilder::from_rect(
                tiny_skia::Rect::from_xywh(
                    0.0,
                    0.0,
                    physical_size.width as f32,
                    physical_size.height as f32,
                )
                .expect("Create damage rectangle"),
            );

            pixels.fill_path(
                &path,
                &tiny_skia::Paint {
                    shader: tiny_skia::Shader::SolidColor(engine::into_color(
                        Color {
                            a: 0.1,
                            ..background_color
                        },
                    )),
                    anti_alias: false,
                    ..Default::default()
                },
                tiny_skia::FillRule::default(),
                tiny_skia::Transform::identity(),
                None,
            );
        }

        self.layers.flush();

        for &region in damage {
            let region = region * scale_factor;

            let path = tiny_skia::PathBuilder::from_rect(
                tiny_skia::Rect::from_xywh(
                    region.x,
                    region.y,
                    region.width,
                    region.height,
                )
                .expect("Create damage rectangle"),
            );

            pixels.fill_path(
                &path,
                &tiny_skia::Paint {
                    shader: tiny_skia::Shader::SolidColor(engine::into_color(
                        background_color,
                    )),
                    anti_alias: false,
                    blend_mode: tiny_skia::BlendMode::Source,
                    ..Default::default()
                },
                tiny_skia::FillRule::default(),
                tiny_skia::Transform::identity(),
                None,
            );

            for layer in self.layers.iter() {
                let Some(clip_bounds) =
                    region.intersection(&(layer.bounds * scale_factor))
                else {
                    continue;
                };

                engine::adjust_clip_mask(clip_mask, clip_bounds);

                for (quad, background) in &layer.quads {
                    self.engine.draw_quad(
                        quad,
                        background,
                        Transformation::scale(scale_factor),
                        pixels,
                        clip_mask,
                        clip_bounds,
                    );
                }

                for group in &layer.primitives {
                    // MultiTunnel patch (S10): `group.clip_bounds()` is in
                    // the group's LOCAL space (canvas `Frame` clip starts
                    // at the origin) — it must be mapped to ABSOLUTE
                    // window space through the group transformation before
                    // it can be intersected with the (absolute) damage
                    // region. Upstream used the raw local clip, so on
                    // partial redraws every canvas outside the window's
                    // top-left corner was masked to a rectangle at the
                    // ORIGIN — its fills/strokes vanished or appeared
                    // displaced ("out of frame") until a coincidental
                    // full-window redraw.
                    let Some(new_clip_bounds) = (group.clip_bounds()
                        * group.transformation()
                        * scale_factor)
                        .intersection(&clip_bounds)
                    else {
                        continue;
                    };

                    engine::adjust_clip_mask(clip_mask, new_clip_bounds);

                    for primitive in group.as_slice() {
                        self.engine.draw_primitive(
                            primitive,
                            // MultiTunnel patch (S10.1): scale AFTER the
                            // group offset — see `group_transform`.
                            group_transform(
                                group.transformation(),
                                scale_factor,
                            ),
                            pixels,
                            clip_mask,
                            clip_bounds,
                        );
                    }

                    engine::adjust_clip_mask(clip_mask, clip_bounds);
                }

                for image in &layer.images {
                    self.engine.draw_image(
                        image,
                        Transformation::scale(scale_factor),
                        pixels,
                        clip_mask,
                        clip_bounds,
                    );
                }

                for group in &layer.text {
                    for text in group.as_slice() {
                        self.engine.draw_text(
                            text,
                            // MultiTunnel patch (S10.1): same order fix as
                            // the primitive groups above — canvas TEXT was
                            // displaced identically on HiDPI.
                            group_transform(
                                group.transformation(),
                                scale_factor,
                            ),
                            pixels,
                            clip_mask,
                            clip_bounds,
                        );
                    }
                }
            }

            if !overlay.is_empty() {
                pixels.stroke_path(
                    &path,
                    &tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(
                            engine::into_color(Color::from_rgb(1.0, 0.0, 0.0)),
                        ),
                        anti_alias: false,
                        ..tiny_skia::Paint::default()
                    },
                    &tiny_skia::Stroke {
                        width: 1.0,
                        ..tiny_skia::Stroke::default()
                    },
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }

        self.engine.trim();
    }
}

impl core::Renderer for Renderer {
    fn start_layer(&mut self, bounds: Rectangle) {
        self.layers.push_clip(bounds);
    }

    fn end_layer(&mut self) {
        self.layers.pop_clip();
    }

    fn start_transformation(&mut self, transformation: Transformation) {
        self.layers.push_transformation(transformation);
    }

    fn end_transformation(&mut self) {
        self.layers.pop_transformation();
    }

    fn fill_quad(
        &mut self,
        quad: renderer::Quad,
        background: impl Into<Background>,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_quad(quad, background.into(), transformation);
    }

    fn clear(&mut self) {
        self.layers.clear();
    }
}

impl core::text::Renderer for Renderer {
    type Font = Font;
    type Paragraph = Paragraph;
    type Editor = Editor;

    const ICON_FONT: Font = Font::with_name("Iced-Icons");
    const CHECKMARK_ICON: char = '\u{f00c}';
    const ARROW_DOWN_ICON: char = '\u{e800}';

    fn default_font(&self) -> Self::Font {
        self.default_font
    }

    fn default_size(&self) -> Pixels {
        self.default_text_size
    }

    fn fill_paragraph(
        &mut self,
        text: &Self::Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();

        layer.draw_paragraph(
            text,
            position,
            color,
            clip_bounds,
            transformation,
        );
    }

    fn fill_editor(
        &mut self,
        editor: &Self::Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_editor(editor, position, color, clip_bounds, transformation);
    }

    fn fill_text(
        &mut self,
        text: core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_text(text, position, color, clip_bounds, transformation);
    }
}

#[cfg(feature = "geometry")]
impl graphics::geometry::Renderer for Renderer {
    type Geometry = Geometry;
    type Frame = geometry::Frame;

    fn new_frame(&self, size: core::Size) -> Self::Frame {
        geometry::Frame::new(size)
    }

    fn draw_geometry(&mut self, geometry: Self::Geometry) {
        let (layer, transformation) = self.layers.current_mut();

        match geometry {
            Geometry::Live {
                primitives,
                images,
                text,
                clip_bounds,
            } => {
                layer.draw_primitive_group(
                    primitives,
                    clip_bounds,
                    transformation,
                );

                for image in images {
                    layer.draw_image(image, transformation);
                }

                layer.draw_text_group(text, clip_bounds, transformation);
            }
            Geometry::Cache(cache) => {
                layer.draw_primitive_cache(
                    cache.primitives,
                    cache.clip_bounds,
                    transformation,
                );

                for image in cache.images.iter() {
                    layer.draw_image(image.clone(), transformation);
                }

                layer.draw_text_cache(
                    cache.text,
                    cache.clip_bounds,
                    transformation,
                );
            }
        }
    }
}

impl graphics::mesh::Renderer for Renderer {
    fn draw_mesh(&mut self, _mesh: graphics::Mesh) {
        log::warn!("iced_tiny_skia does not support drawing meshes");
    }
}

#[cfg(feature = "image")]
impl core::image::Renderer for Renderer {
    type Handle = core::image::Handle;

    fn measure_image(&self, handle: &Self::Handle) -> crate::core::Size<u32> {
        self.engine.raster_pipeline.dimensions(handle)
    }

    fn draw_image(&mut self, image: core::Image, bounds: Rectangle) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_raster(image, bounds, transformation);
    }
}

#[cfg(feature = "svg")]
impl core::svg::Renderer for Renderer {
    fn measure_svg(
        &self,
        handle: &core::svg::Handle,
    ) -> crate::core::Size<u32> {
        self.engine.vector_pipeline.viewport_dimensions(handle)
    }

    fn draw_svg(&mut self, svg: core::Svg, bounds: Rectangle) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_svg(svg, bounds, transformation);
    }
}

impl compositor::Default for Renderer {
    type Compositor = window::Compositor;
}

#[cfg(test)]
mod multitunnel_tests {
    use super::*;
    use crate::core::Size;

    /// MultiTunnel S10.1 regression: the composed group transform must map
    /// LOCAL canvas coordinates to PHYSICAL pixels as
    /// `(local + offset) · sf`. Upstream composed `group * scale`, which
    /// under glam semantics scales first and adds the UNSCALED offset
    /// (`local·sf + offset`) — every canvas at a non-identity scale factor
    /// was displaced `offset·(sf − 1)` px up-left of its widget bounds
    /// (the v0.6.0 HiDPI "graph axes jump out of bounds" bug).
    #[test]
    fn group_transform_scales_the_offset_too() {
        // A canvas card whose top-left sits at logical (200, 300) in a
        // 150%-scaled viewport.
        let group = Transformation::translate(200.0, 300.0);
        let sf = 1.5;

        let t = group_transform(group, sf);

        // Local origin -> physical card origin: (200, 300) * 1.5.
        let origin = Point::ORIGIN * t;
        assert_eq!(origin, Point::new(300.0, 450.0));

        // A point 100 px into the canvas.
        let inside = Point::new(100.0, 40.0) * t;
        assert_eq!(inside, Point::new(450.0, 510.0));

        // The scale factor must survive the composition (glyph
        // rasterization reads `transformation.scale_factor()`).
        assert_eq!(t.scale_factor(), sf);

        // The WRONG order (upstream) for contrast: local·sf + offset.
        let wrong = group * Transformation::scale(sf);
        let wrong_origin = Point::ORIGIN * wrong;
        assert_eq!(wrong_origin, Point::new(200.0, 300.0));
        assert_ne!(wrong_origin, origin);
    }

    /// At scale factor 1 both orders coincide — the fix must not change
    /// 100%-DPI rendering.
    #[test]
    fn group_transform_identity_at_scale_one() {
        let group = Transformation::translate(123.0, 456.0);
        assert_eq!(group_transform(group, 1.0), group);
    }

    /// The render-loop CLIP mapping (`rect * group * scale`, applied
    /// left-to-right = translate first, scale second) must agree with
    /// `group_transform` — the clip and the content it clips must be in
    /// the same space.
    #[test]
    fn clip_mapping_agrees_with_group_transform() {
        let group = Transformation::translate(200.0, 300.0);
        let sf = 1.5;
        let local_clip = Rectangle::new(Point::ORIGIN, Size::new(400.0, 200.0));

        let clip = local_clip * group * Transformation::scale(sf);
        let content_origin = Point::ORIGIN * group_transform(group, sf);

        assert_eq!(
            (clip.x, clip.y),
            (content_origin.x, content_origin.y),
            "clip and content origins must coincide: {clip:?} vs {content_origin:?}"
        );
        assert_eq!((clip.width, clip.height), (600.0, 300.0));
    }
}
