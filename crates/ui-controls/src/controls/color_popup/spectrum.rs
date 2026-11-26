use rust_desktop_ui_core::{Bitmap, Color, ImagePrimitive, Point, Rect, Scene, Srgba8};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpectrumPart {
    Plane,
    Hue,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Default for Hsv {
    fn default() -> Self {
        Self {
            h: 0.,
            s: 1.,
            v: 1.,
        }
    }
}

impl Hsv {
    /// RGB cannot express a hue for grey or a saturation for black.
    pub fn from_color(color: Srgba8, previous: Self) -> Self {
        let [r, g, b] = [color.r, color.g, color.b].map(f32::from);
        let max = r.max(g).max(b);
        let chroma = max - r.min(g).min(b);
        let v = max / 255.;
        if chroma == 0. {
            return Self {
                h: previous.h,
                s: if max == 0. { previous.s } else { 0. },
                v,
            };
        }
        let sector = if max == r {
            (g - b) / chroma
        } else if max == g {
            (b - r) / chroma + 2.
        } else {
            (r - g) / chroma + 4.
        };
        let h = (sector / 6.).rem_euclid(1.);
        // Red occupies both ends of the hue strip. Keep the nearer end selected.
        let h = if h == 0. && previous.h > 0.5 { 1. } else { h };
        Self {
            h,
            s: chroma / max,
            v,
        }
    }

    pub fn color(self, alpha: u8) -> Srgba8 {
        let hue = self.h.clamp(0., 1.) * 6.;
        let saturation = self.s.clamp(0., 1.);
        let value = self.v.clamp(0., 1.);
        let chroma = value * saturation;
        let middle = chroma * (1. - (hue % 2. - 1.).abs());
        let rgb = match hue as u8 {
            0 | 6 => [chroma, middle, 0.],
            1 => [middle, chroma, 0.],
            2 => [0., chroma, middle],
            3 => [0., middle, chroma],
            4 => [middle, 0., chroma],
            _ => [chroma, 0., middle],
        };
        let [r, g, b] = rgb.map(|c| ((c + (value - chroma)) * 255.).round() as u8);
        Srgba8::new(r, g, b, alpha)
    }

    pub fn at(self, part: SpectrumPart, bounds: Rect, point: Point) -> Self {
        if !bounds.is_valid() || bounds.width == 0. || bounds.height == 0. || !point.is_finite() {
            return self;
        }
        let y = ((point.y - bounds.y) / bounds.height).clamp(0., 1.);
        match part {
            SpectrumPart::Plane => Self {
                s: ((point.x - bounds.x) / bounds.width).clamp(0., 1.),
                v: 1. - y,
                ..self
            },
            SpectrumPart::Hue => Self { h: y, ..self },
        }
    }
}

#[derive(Default, Debug, Clone)]
pub(super) struct SpectrumRaster {
    plane: Option<(Srgba8, Bitmap)>,
    hue_strip: Option<Bitmap>,
}

impl SpectrumRaster {
    pub fn paint(
        &mut self,
        scene: &mut Scene,
        part: SpectrumPart,
        hsv: Hsv,
        bounds: Rect,
        clip: Rect,
    ) {
        let Some(clip) = clip.intersection(bounds) else {
            return;
        };
        let (bitmap, marker) = match part {
            SpectrumPart::Plane => {
                let pure = Hsv {
                    h: hsv.h,
                    ..Hsv::default()
                }
                .color(255);
                if self.plane.as_ref().is_none_or(|(key, _)| *key != pure) {
                    let bitmap = raster(65, 65, |x, y| {
                        let mix =
                            |c: u8| (((1. - x) * 255. + x * f32::from(c)) * (1. - y)).round() as u8;
                        Srgba8::rgb(mix(pure.r), mix(pure.g), mix(pure.b))
                    });
                    self.plane = Some((pure, bitmap));
                }
                let diameter = 12_f32.min(bounds.width).min(bounds.height);
                let marker = Rect::new(
                    bounds.x + marker_start(hsv.s, bounds.width, diameter),
                    bounds.y + marker_start(1. - hsv.v, bounds.height, diameter),
                    diameter,
                    diameter,
                );
                (self.plane.as_ref().unwrap().1.clone(), marker)
            }
            SpectrumPart::Hue => {
                let strip = self.hue_strip.get_or_insert_with(|| {
                    raster(1, 193, |_, h| {
                        Hsv {
                            h,
                            ..Hsv::default()
                        }
                        .color(255)
                    })
                });
                let height = 6_f32.min(bounds.height);
                let marker = Rect::new(
                    bounds.x,
                    bounds.y + marker_start(hsv.h, bounds.height, height),
                    bounds.width,
                    height,
                );
                (strip.clone(), marker)
            }
        };
        let mut image = ImagePrimitive::new(bitmap, bounds);
        image.clip = clip;
        scene.image(image);
        // A white ring with a dark outline stays visible over either extreme.
        let dark = Color::rgb(45, 45, 45);
        let light = Color::rgb(255, 255, 255);
        match part {
            SpectrumPart::Plane => {
                scene.ellipse(marker, clip, Some(3.), dark);
                scene.ellipse(marker, clip, Some(1.5), light);
            }
            SpectrumPart::Hue => {
                scene.rounded_stroke(marker, clip, 2., 2.5, dark);
                scene.rounded_stroke(marker, clip, 2., 1., light);
            }
        }
    }
}

fn marker_start(fraction: f32, length: f32, marker: f32) -> f32 {
    (fraction * length - marker / 2.).clamp(0., length - marker)
}

fn raster(width: u32, height: u32, sample: impl Fn(f32, f32) -> Srgba8) -> Bitmap {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let c = sample(
                x as f32 / (width - 1).max(1) as f32,
                y as f32 / (height - 1).max(1) as f32,
            );
            pixels.extend_from_slice(&[c.r, c.g, c.b, c.a]);
        }
    }
    Bitmap::new_rgba8(width, height, pixels).expect("fixed-size RGBA spectrum bitmap")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hue_sectors_and_transparent_rgb_round_trip() {
        let colors = [
            (255, 0, 0),
            (255, 255, 0),
            (0, 255, 0),
            (0, 255, 255),
            (0, 0, 255),
            (255, 0, 255),
            (255, 0, 0),
        ];
        for (i, (r, g, b)) in colors.into_iter().enumerate() {
            let hsv = Hsv {
                h: i as f32 / 6.,
                ..Hsv::default()
            };
            assert_eq!(hsv.color(81), Srgba8::new(r, g, b, 81));
            assert_eq!(Hsv::from_color(hsv.color(0), hsv), hsv);
        }
        for r in [0, 1, 35, 127, 254, 255] {
            for g in [0, 1, 36, 127, 254, 255] {
                for b in [0, 1, 37, 127, 254, 255] {
                    let transparent = Srgba8::new(r, g, b, 0);
                    assert_eq!(
                        Hsv::from_color(transparent, Hsv::default()).color(0),
                        transparent
                    );
                }
            }
        }
    }

    #[test]
    fn grey_retains_hue_and_black_also_retains_saturation() {
        let starting = Hsv {
            h: 0.25,
            s: 0.75,
            v: 0.5,
        };
        let grey = Hsv::from_color(Srgba8::rgb(89, 89, 89), starting);
        assert_eq!((grey.h, grey.s), (0.25, 0.));
        assert_eq!(grey.color(37), Srgba8::new(89, 89, 89, 37));
        let black = Hsv::from_color(Srgba8::BLACK, starting);
        assert_eq!((black.h, black.s, black.v), (0.25, 0.75, 0.));
        assert_eq!(
            Hsv {
                v: starting.v,
                ..black
            }
            .color(255),
            starting.color(255)
        );
    }

    #[test]
    fn drag_uses_local_coordinates_and_clamps_to_edges() {
        let bounds = Rect::new(40., 10., 80., 50.);
        let initial = Hsv {
            h: 0.5,
            s: 0.3,
            v: 0.4,
        };
        let along_plane = |x, y| initial.at(SpectrumPart::Plane, bounds, Point::new(x, y));
        assert_eq!(along_plane(0., -15.).color(255), Srgba8::WHITE);
        assert_eq!(along_plane(135., 0.).color(255), Srgba8::rgb(0, 255, 255));
        assert_eq!(along_plane(140., 65.).color(255), Srgba8::BLACK);
        assert_eq!(
            (along_plane(60., 35.).s, along_plane(60., 35.).v),
            (0.25, 0.5)
        );
        let hue = initial.at(SpectrumPart::Hue, bounds, Point::new(0., 60.));
        assert_eq!((hue.h, hue.s, hue.v), (1., initial.s, initial.v));
        assert_eq!(
            initial.at(SpectrumPart::Plane, Rect::new(0., 0., 0., 0.), Point::ZERO),
            initial
        );
        assert_eq!(
            initial.at(SpectrumPart::Plane, bounds, Point::new(f32::NAN, 12.)),
            initial
        );
    }

    #[test]
    fn raster_caches_color_plane_and_strip_independently_of_marker() {
        let bounds = Rect::new(15., 5., 180., 100.);
        let clip = Rect::new(15., 5., 140., 80.);
        let mut raster = SpectrumRaster::default();
        let mut scene = Scene::default();
        let red = Hsv::default();
        raster.paint(&mut scene, SpectrumPart::Plane, red, bounds, clip);
        let bitmap = scene.images[0].bitmap.clone();
        assert_eq!(scene.images[0].clip, clip);
        let side = bitmap.width() as usize;
        let pixels = bitmap.pixels();
        assert_eq!(&pixels[..4], &[255, 255, 255, 255]);
        assert_eq!(&pixels[side * 4 - 4..side * 4], &[255, 0, 0, 255]);
        assert_eq!(&pixels[pixels.len() - 4..], &[0, 0, 0, 255]);
        for frac in [0., 0.5, 1.] {
            let hsv = Hsv {
                s: frac,
                v: 1. - frac,
                ..red
            };
            scene.clear();
            raster.paint(&mut scene, SpectrumPart::Plane, hsv, bounds, bounds);
            assert_eq!(scene.images[0].bitmap, bitmap);
            let marker = scene.ellipses[0].bounds;
            assert_eq!(marker.intersection(bounds), Some(marker));
        }
        let blue = Hsv { h: 2. / 3., ..red };
        raster.paint(&mut scene, SpectrumPart::Plane, blue, bounds, clip);
        assert_ne!(scene.images.last().unwrap().bitmap, bitmap);
        raster.paint(&mut scene, SpectrumPart::Hue, red, bounds, clip);
        let strip = scene.images.last().unwrap().bitmap.clone();
        raster.paint(&mut scene, SpectrumPart::Hue, blue, bounds, clip);
        assert_eq!(scene.images.last().unwrap().bitmap, strip);
        let pixels = strip.pixels();
        assert_eq!(&pixels[..4], &pixels[pixels.len() - 4..]);
    }
}
