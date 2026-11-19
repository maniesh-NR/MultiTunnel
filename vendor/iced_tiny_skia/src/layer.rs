use crate::core::renderer::Quad;
use crate::core::{
    self, Background, Color, Point, Rectangle, Svg, Transformation,
};
use crate::graphics::damage;
use crate::graphics::layer;
use crate::graphics::text::{Editor, Paragraph, Text};
use crate::graphics::{self, Image};
use crate::Primitive;

use std::rc::Rc;

pub type Stack = layer::Stack<Layer>;

#[derive(Debug, Clone)]
pub struct Layer {
    pub bounds: Rectangle,
    pub quads: Vec<(Quad, Background)>,
    pub primitives: Vec<Item<Primitive>>,
    pub text: Vec<Item<Text>>,
    pub images: Vec<Image>,
}

impl Layer {
    pub fn draw_quad(
        &mut self,
        mut quad: Quad,
        background: Background,
        transformation: Transformation,
    ) {
        quad.bounds = quad.bounds * transformation;
        self.quads.push((quad, background));
    }

    pub fn draw_paragraph(
        &mut self,
        paragraph: &Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let paragraph = Text::Paragraph {
            paragraph: paragraph.downgrade(),
            position,
            color,
            clip_bounds,
            transformation,
        };

        self.text.push(Item::Live(paragraph));
    }

    pub fn draw_editor(
        &mut self,
        editor: &Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let editor = Text::Editor {
            editor: editor.downgrade(),
            position,
            color,
            clip_bounds,
            transformation,
        };

        self.text.push(Item::Live(editor));
    }

    pub fn draw_text(
        &mut self,
        text: core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        let text = Text::Cached {
            content: text.content,
            bounds: Rectangle::new(position, text.bounds) * transformation,
            color,
            size: text.size * transformation.scale_factor(),
            line_height: text.line_height.to_absolute(text.size)
                * transformation.scale_factor(),
            font: text.font,
            horizontal_alignment: text.horizontal_alignment,
            vertical_alignment: text.vertical_alignment,
            shaping: text.shaping,
            clip_bounds: clip_bounds * transformation,
        };

        self.text.push(Item::Live(text));
    }

    pub fn draw_text_group(
        &mut self,
        text: Vec<Text>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.text
            .push(Item::Group(text, clip_bounds, transformation));
    }

    pub fn draw_text_cache(
        &mut self,
        text: Rc<[Text]>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.text
            .push(Item::Cached(text, clip_bounds, transformation));
    }

    pub fn draw_image(&mut self, image: Image, transformation: Transformation) {
        match image {
            Image::Raster(raster, bounds) => {
                self.draw_raster(raster, bounds, transformation);
            }
            Image::Vector(svg, bounds) => {
                self.draw_svg(svg, bounds, transformation);
            }
        }
    }

    pub fn draw_raster(
        &mut self,
        image: core::Image,
        bounds: Rectangle,
        transformation: Transformation,
    ) {
        let image = Image::Raster(image, bounds * transformation);

        self.images.push(image);
    }

    pub fn draw_svg(
        &mut self,
        svg: Svg,
        bounds: Rectangle,
        transformation: Transformation,
    ) {
        let svg = Image::Vector(svg, bounds * transformation);

        self.images.push(svg);
    }

    pub fn draw_primitive_group(
        &mut self,
        primitives: Vec<Primitive>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.primitives.push(Item::Group(
            primitives,
            clip_bounds,
            transformation,
        ));
    }

    pub fn draw_primitive_cache(
        &mut self,
        primitives: Rc<[Primitive]>,
        clip_bounds: Rectangle,
        transformation: Transformation,
    ) {
        self.primitives.push(Item::Cached(
            primitives,
            clip_bounds,
            transformation,
        ));
    }

    pub fn damage(previous: &Self, current: &Self) -> Vec<Rectangle> {
        if previous.bounds != current.bounds {
            return vec![previous.bounds, current.bounds];
        }

        let mut damage = damage::list(
            &previous.quads,
            &current.quads,
            |(quad, _)| {
                quad.bounds
                    .expand(1.0)
                    .intersection(&current.bounds)
                    .into_iter()
                    .collect()
            },
            |(quad_a, background_a), (quad_b, background_b)| {
                quad_a == quad_b && background_a == background_b
            },
        );

        let text = damage::diff(
            &previous.text,
            &current.text,
            |item| {
                item.as_slice()
                    .iter()
                    .filter_map(Text::visible_bounds)
                    .map(|bounds| bounds * item.transformation())
                    .collect()
            },
            |text_a, text_b| {
                damage::list(
                    text_a.as_slice(),
                    text_b.as_slice(),
                    |text| {
                        text.visible_bounds()
                            .into_iter()
                            .map(|bounds| bounds * text_a.transformation())
                            .collect()
                    },
                    |text_a, text_b| text_a == text_b,
                )
            },
        );

        let primitives = damage::list(
            &previous.primitives,
            &current.primitives,
            |item| match item {
                Item::Live(primitive) => vec![primitive.visible_bounds()],
                Item::Group(primitives, group_bounds, transformation) => {
                    // MultiTunnel patch (S10): `group_bounds` is stored in
                    // the GROUP's LOCAL space (a canvas `Frame` clips to
                    // `Rectangle::new(ORIGIN, size)`), while the primitive
                    // bounds above are mapped to ABSOLUTE window space by
                    // `bounds * transformation`. Upstream intersected the
                    // absolute bounds with the UNTRANSFORMED local clip, so
                    // every canvas not at the window origin produced an
                    // EMPTY intersection — zero damage — and its fills and
                    // strokes were never repainted (the v0.5.1 "graphs
                    // don't load / appear out of frame" bug: series and
                    // grid went blank while canvas TEXT — which is not
                    // clipped in this closure — kept painting). Transform
                    // the clip into absolute space first.
                    let clip = *group_bounds * *transformation;
                    primitives
                        .as_slice()
                        .iter()
                        .map(Primitive::visible_bounds)
                        .map(|bounds| bounds * *transformation)
                        .filter_map(|bounds| bounds.intersection(&clip))
                        .collect()
                }
                // MultiTunnel patch (S10): same space mismatch — a cached
                // group's bounds are LOCAL; damage must report them in
                // ABSOLUTE space or the wrong window region is repainted.
                Item::Cached(_, bounds, transformation) => {
                    vec![*bounds * *transformation]
                }
            },
            |primitive_a, primitive_b| match (primitive_a, primitive_b) {
                (
                    Item::Cached(cache_a, bounds_a, transformation_a),
                    Item::Cached(cache_b, bounds_b, transformation_b),
                ) => {
                    Rc::ptr_eq(cache_a, cache_b)
                        && bounds_a == bounds_b
                        && transformation_a == transformation_b
                }
                _ => false,
            },
        );

        let images = damage::list(
            &previous.images,
            &current.images,
            |image| vec![image.bounds().expand(1.0)],
            Image::eq,
        );

        damage.extend(text);
        damage.extend(primitives);
        damage.extend(images);
        damage
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            bounds: Rectangle::INFINITE,
            quads: Vec::new(),
            primitives: Vec::new(),
            text: Vec::new(),
            images: Vec::new(),
        }
    }
}

impl graphics::Layer for Layer {
    fn with_bounds(bounds: Rectangle) -> Self {
        Self {
            bounds,
            ..Self::default()
        }
    }

    fn flush(&mut self) {}

    fn resize(&mut self, bounds: Rectangle) {
        self.bounds = bounds;
    }

    fn reset(&mut self) {
        self.bounds = Rectangle::INFINITE;

        self.quads.clear();
        self.primitives.clear();
        self.text.clear();
        self.images.clear();
    }
}

#[derive(Debug, Clone)]
pub enum Item<T> {
    Live(T),
    Group(Vec<T>, Rectangle, Transformation),
    Cached(Rc<[T]>, Rectangle, Transformation),
}

impl<T> Item<T> {
    pub fn transformation(&self) -> Transformation {
        match self {
            Item::Live(_) => Transformation::IDENTITY,
            Item::Group(_, _, transformation)
            | Item::Cached(_, _, transformation) => *transformation,
        }
    }

    pub fn clip_bounds(&self) -> Rectangle {
        match self {
            Item::Live(_) => Rectangle::INFINITE,
            Item::Group(_, clip_bounds, _)
            | Item::Cached(_, clip_bounds, _) => *clip_bounds,
        }
    }

    pub fn as_slice(&self) -> &[T] {
        match self {
            Item::Live(item) => std::slice::from_ref(item),
            Item::Group(group, _, _) => group.as_slice(),
            Item::Cached(cache, _, _) => cache,
        }
    }
}

#[cfg(test)]
mod multitunnel_tests {
    use super::*;
    use crate::core::{Point, Rectangle, Size, Transformation};

    fn fill_primitive(x: f32, y: f32, w: f32, h: f32) -> Primitive {
        let path = tiny_skia::PathBuilder::from_rect(
            tiny_skia::Rect::from_xywh(x, y, w, h).expect("valid rect"),
        );
        Primitive::Fill {
            path,
            paint: tiny_skia::Paint::default(),
            rule: tiny_skia::FillRule::Winding,
        }
    }

    fn layer_with_group(clip: Rectangle, translation: (f32, f32)) -> Layer {
        let mut layer = Layer::default();
        layer.bounds = Rectangle::new(Point::ORIGIN, Size::new(1280.0, 800.0));
        layer.primitives.push(Item::Group(
            vec![fill_primitive(0.0, 0.0, 100.0, 50.0)],
            clip,
            Transformation::translate(translation.0, translation.1),
        ));
        layer
    }

    /// MultiTunnel S10 regression: a canvas `Frame` records its clip in
    /// LOCAL space (origin-based) while the widget pushes an ABSOLUTE
    /// translation onto the layer stack. The damage closure must therefore
    /// map the group clip through the transformation before intersecting
    /// the (already absolute) primitive bounds — upstream used the raw
    /// local clip, so any canvas away from the window origin produced ZERO
    /// damage and never repainted (v0.5.1 "graphs don't load").
    #[test]
    fn group_damage_is_reported_in_absolute_space() {
        // Canvas card at absolute (200, 300); local clip is origin-based
        // 400x200; the series fills the local rect.
        let clip = Rectangle::new(Point::ORIGIN, Size::new(400.0, 200.0));
        let previous = layer_with_group(clip, (200.0, 300.0));

        // "Changed" frame: same geometry, different paint color is not
        // observable here — instead move the fill slightly; either way the
        // items are never equal, so damage must list the primitive bounds
        // INTERSECTED with the ABSOLUTE clip (200..600 x 300..500).
        let mut current = layer_with_group(clip, (200.0, 300.0));
        current.primitives[0] = Item::Group(
            vec![fill_primitive(10.0, 0.0, 100.0, 50.0)],
            clip,
            Transformation::translate(200.0, 300.0),
        );

        let damage = Layer::damage(&previous, &current);

        assert!(
            !damage.is_empty(),
            "a changed canvas group must produce damage"
        );
        for rect in &damage {
            // Absolute space: everything must land inside the card area,
            // not around the window origin.
            assert!(
                rect.x >= 200.0 && rect.y >= 300.0,
                "damage rect {rect:?} is not in the canvas' absolute space"
            );
        }
    }

    /// Cached geometry damage must ALSO be reported in absolute space.
    #[test]
    fn cached_damage_is_reported_in_absolute_space() {
        let clip = Rectangle::new(Point::ORIGIN, Size::new(400.0, 200.0));
        let mut previous = Layer::default();
        previous.bounds = Rectangle::new(Point::ORIGIN, Size::new(1280.0, 800.0));
        previous.primitives.push(Item::Cached(
            std::rc::Rc::from(vec![fill_primitive(0.0, 0.0, 100.0, 50.0)]),
            clip,
            Transformation::translate(200.0, 300.0),
        ));

        // Different Rc identity → never equal → damage must be the bounds
        // MAPPED through the transformation (absolute), not the raw local
        // rect at the origin.
        let mut current = Layer::default();
        current.bounds = Rectangle::new(Point::ORIGIN, Size::new(1280.0, 800.0));
        current.primitives.push(Item::Cached(
            std::rc::Rc::from(vec![fill_primitive(5.0, 0.0, 100.0, 50.0)]),
            clip,
            Transformation::translate(200.0, 300.0),
        ));

        let damage = Layer::damage(&previous, &current);
        assert_eq!(damage.len(), 2, "both caches changed: {damage:?}");
        for rect in &damage {
            assert!(
                rect.x >= 200.0 && rect.y >= 300.0,
                "cached damage rect {rect:?} must be absolute"
            );
        }
    }
}
