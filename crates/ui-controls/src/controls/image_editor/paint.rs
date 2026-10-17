use super::*;
impl Controls {
    pub(super) fn image_editor_mapping(&self, id: WidgetId) -> Option<(Rect, f32)> {
        let e = self.image_editors.entries.get(&id)?;
        let image = e.session.draft()?;
        let b = self
            .runtime
            .bounds(e.parts.viewport)?
            .intersection(self.runtime.clip(e.parts.viewport)?)?;
        if b.width <= 0.0 || b.height <= 0.0 {
            return None;
        }
        let scale = if e.view.fit {
            (b.width / image.width() as f32)
                .min(b.height / image.height() as f32)
                .clamp(0.0001, 32.0)
        } else {
            e.view.zoom
        };
        let w = image.width() as f32 * scale;
        let h = image.height() as f32 * scale;
        Some((
            Rect::new(
                b.x + (b.width - w) * 0.5 + e.view.pan.x,
                b.y + (b.height - h) * 0.5 + e.view.pan.y,
                w,
                h,
            ),
            scale,
        ))
    }
    pub(in crate::controls) fn paint_image_editor(
        &mut self,
        part: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> bool {
        let Some(id) = self
            .image_editors
            .entries
            .iter()
            .find_map(|(id, e)| (e.parts.viewport == part).then_some(*id))
        else {
            return false;
        };
        let Some(clip) = bounds.intersection(clip) else {
            return true;
        };
        let cell = (clip.width / 64.0).max(clip.height / 64.0).max(16.0);
        let colors = [Color::rgb(199, 199, 199), Color::rgb(230, 230, 230)];
        for y in 0..(clip.height / cell).ceil() as usize {
            for x in 0..(clip.width / cell).ceil() as usize {
                fill_clipped(
                    &mut self.scene,
                    Rect::new(
                        clip.x + x as f32 * cell,
                        clip.y + y as f32 * cell,
                        cell,
                        cell,
                    ),
                    clip,
                    colors[(x + y) % 2],
                );
            }
        }
        if let Some((mapping, scale)) = self.image_editor_mapping(id) {
            let e = &self.image_editors.entries[&id];
            let mut image = ImagePrimitive::new(e.session.draft().unwrap().clone(), mapping);
            image.clip = clip;
            image.sampling = ImageSampling::Nearest;
            self.scene.image(image);
            if let Some(crop) = e.crop {
                let r = Rect::new(
                    mapping.x + crop.x as f32 * scale,
                    mapping.y + crop.y as f32 * scale,
                    crop.width as f32 * scale,
                    crop.height as f32 * scale,
                );
                let color = self.theme.tokens().focus;
                for line in [
                    Rect::new(r.x, r.y, r.width, 2.0),
                    Rect::new(r.x, r.y + r.height - 2.0, r.width, 2.0),
                    Rect::new(r.x, r.y, 2.0, r.height),
                    Rect::new(r.x + r.width - 2.0, r.y, 2.0, r.height),
                ] {
                    fill_clipped(&mut self.scene, line, clip, color);
                }
            }
        }
        true
    }
}
