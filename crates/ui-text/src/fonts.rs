//! System font metadata uses the same collection as paragraph shaping.
use crate::{TextEngine, TextError};
use rust_desktop_ui_core::{FontDescriptor, FontSlant};

#[derive(Clone, Debug, PartialEq)]
pub struct FontFaceInfo {
    pub weight: u16,
    pub width: f32,
    pub slant: FontSlant,
}
impl FontFaceInfo {
    pub fn descriptor(&self, family: impl Into<String>, size: f32) -> FontDescriptor {
        FontDescriptor {
            family: family.into(),
            size,
            weight: self.weight,
            width: self.width,
            slant: self.slant,
        }
    }
    pub fn label(&self) -> String {
        let style = match self.slant {
            FontSlant::Normal => "Normal".into(),
            FontSlant::Italic => "Italic".into(),
            FontSlant::Oblique(angle) => format!("Oblique {angle}°"),
        };
        format!("{style} · {} · {}%", self.weight, self.width)
    }
}
impl TextEngine {
    /// Installed family names, sorted and deduplicated. This is a snapshot;
    /// create a new engine to discover fonts installed later in the process.
    pub fn font_families(&mut self) -> Result<Vec<String>, TextError> {
        let mut names: Vec<String> = self
            .fonts
            .collection
            .family_names()
            .take(16_385)
            .map(str::to_owned)
            .collect();
        if names.len() > 16_384 {
            return Err(TextError::FontCatalogLimit);
        }
        names.retain(|name| {
            FontDescriptor {
                family: name.clone(),
                ..Default::default()
            }
            .is_valid()
        });
        names.sort_by_cached_key(|name| name.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        Ok(names)
    }
    /// Physical face attributes reported by the installed font resource.
    /// Missing families return an empty list; synthetic variants are not added.
    pub fn font_faces(&mut self, family: &str) -> Result<Vec<FontFaceInfo>, TextError> {
        if !(FontDescriptor {
            family: family.into(),
            ..Default::default()
        })
        .is_valid()
        {
            return Err(TextError::InvalidRun);
        }
        let Some(info) = self.fonts.collection.family_by_name(family.trim()) else {
            return Ok(Vec::new());
        };
        if info.fonts().len() > 1024 {
            return Err(TextError::FontCatalogLimit);
        }
        let mut faces = Vec::new();
        for font in info.fonts() {
            let face = FontFaceInfo {
                weight: font.weight().value().round().clamp(1.0, 1000.0) as u16,
                width: font.width().percentage(),
                slant: match font.style() {
                    parley::FontStyle::Normal => FontSlant::Normal,
                    parley::FontStyle::Italic => FontSlant::Italic,
                    parley::FontStyle::Oblique(angle) => FontSlant::Oblique(angle.unwrap_or(14.0)),
                },
            };
            if face.descriptor(family, 14.0).is_valid() && !faces.contains(&face) {
                faces.push(face);
            }
        }
        Ok(faces)
    }
}
pub(crate) fn slant(value: FontSlant) -> parley::FontStyle {
    match value {
        FontSlant::Normal => parley::FontStyle::Normal,
        FontSlant::Italic => parley::FontStyle::Italic,
        FontSlant::Oblique(angle) => parley::FontStyle::Oblique(Some(angle)),
    }
}
pub(crate) fn font_key(font: &FontDescriptor) -> (String, u32, u16, u32, u8, u32) {
    let (kind, angle) = match font.slant {
        FontSlant::Normal => (0, 0),
        FontSlant::Italic => (1, 0),
        FontSlant::Oblique(angle) => (2, angle.to_bits()),
    };
    (
        font.family.clone(),
        font.size.to_bits(),
        font.weight,
        font.width.to_bits(),
        kind,
        angle,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{Color, Rect, TextRun, TextSpan};
    #[test]
    fn installed_faces_shape_with_descriptor_and_distinct_cache_keys() {
        let mut engine = TextEngine::new();
        let families = engine.font_families().unwrap();
        assert!(!families.is_empty());
        let family = families
            .iter()
            .find(|f| f.as_str() == "Arial")
            .unwrap_or(&families[0]);
        let faces = engine.font_faces(family).unwrap();
        assert!(!faces.is_empty());
        let run = TextRun::new(
            "Font preview 123",
            Rect::new(0.0, 0.0, 800.0, 120.0),
            Color::rgb(0, 0, 0),
            14.0,
        );
        let mut span = TextSpan {
            range: 0..run.text.len(),
            font: Some(faces[0].descriptor(family, 24.0)),
            font_size: None,
            weight: None,
            color: None,
        };
        let first = engine.prepare_styled(&run, &[span.clone()], 1.0).unwrap();
        assert!(!first.font_runs().is_empty());
        assert!(
            first
                .font_runs()
                .iter()
                .all(|r| (r.font_size - 24.0).abs() < 0.01)
        );
        span.font.as_mut().unwrap().size = 40.0;
        let second = engine.prepare_styled(&run, &[span.clone()], 1.0).unwrap();
        assert!(second.width > first.width);
        span.font.as_mut().unwrap().family = "Definitely missing family 827462".into();
        let fallback = engine.prepare_styled(&run, &[span.clone()], 1.0).unwrap();
        assert!(!fallback.font_runs().is_empty());
        assert!(
            engine
                .font_faces(&span.font.as_ref().unwrap().family)
                .unwrap()
                .is_empty()
        );
        span.font.as_mut().unwrap().width = f32::NAN;
        assert!(engine.prepare_styled(&run, &[span], 1.0).is_err());
    }

    #[test]
    fn synthetic_slant_changes_raster_without_aliasing_the_upright_cache() {
        let mut engine = TextEngine::new();
        let families = engine.font_families().unwrap();
        let family = families
            .iter()
            .find(|f| f.as_str() == "Segoe UI Symbol")
            .unwrap_or(&families[0]);
        let run = TextRun::new(
            "ABC",
            rust_desktop_ui_core::Rect::new(0.0, 0.0, 400.0, 100.0),
            rust_desktop_ui_core::Color::rgb(0, 0, 0),
            48.0,
        );
        let font = FontDescriptor {
            family: family.clone(),
            size: 48.0,
            ..Default::default()
        };
        let mut span = rust_desktop_ui_core::TextSpan {
            range: 0..3,
            font: Some(font),
            font_size: None,
            weight: None,
            color: None,
        };
        let upright = engine.prepare_styled(&run, &[span.clone()], 1.0).unwrap();
        span.font.as_mut().unwrap().slant = FontSlant::Oblique(20.0);
        let oblique = engine.prepare_styled(&run, &[span.clone()], 1.0).unwrap();
        assert_eq!(upright.glyphs.len(), oblique.glyphs.len());
        assert!(
            upright
                .glyphs
                .iter()
                .zip(&oblique.glyphs)
                .any(|(a, b)| a.key != b.key)
        );
        assert!(
            upright
                .glyphs
                .iter()
                .zip(&oblique.glyphs)
                .any(|(a, b)| a.bitmap.rgba != b.bitmap.rgba)
        );
        span.font.as_mut().unwrap().slant = FontSlant::Normal;
        let again = engine.prepare_styled(&run, &[span], 1.0).unwrap();
        assert_eq!(upright.glyphs[0].bitmap.rgba, again.glyphs[0].bitmap.rgba);
    }
}
