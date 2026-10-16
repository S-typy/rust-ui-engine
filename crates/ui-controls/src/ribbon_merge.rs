//! Explicit provenance for contextual document Ribbon composition.
use super::*;

impl Ribbon {
    /// Compose tabs from one named source. Matching tab IDs append groups only
    /// when label, KeyTip and context agree; duplicate groups or KeyTips fail.
    /// QAT and command state remain independently owned by the application.
    pub fn merge_tabs(
        &mut self,
        ui: &mut Controls,
        source: &str,
        tabs: Vec<RibbonTab>,
    ) -> Result<(), ControlError> {
        if source.is_empty()
            || source.len() > 128
            || !source
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
            || tabs.is_empty()
            || tabs.len() > 64
            || tabs.iter().map(|tab| tab.groups.len()).sum::<usize>() > 256
            || tabs
                .iter()
                .flat_map(|tab| &tab.groups)
                .map(|group| group.items.len())
                .sum::<usize>()
                > 1024
        {
            return Err(ControlError::Invalid(
                "Ribbon merge requires a bounded source identifier and nonempty tabs".into(),
            ));
        }
        for item in tabs
            .iter()
            .flat_map(|tab| &tab.groups)
            .flat_map(|group| &group.items)
        {
            if !ui.commands().contains(&item.command)
                || matches!(item.kind, RibbonItemKind::Toggle)
                    && ui.commands().checked(&item.command).is_none()
            {
                return Err(ControlError::Invalid("Merged Ribbon commands must be registered; toggles need explicit checked state".into()));
            }
        }
        let mut sources = self.merge_sources.clone();
        if let Some(entry) = sources.iter_mut().find(|entry| entry.0 == source) {
            entry.1 = tabs;
        } else {
            if sources.len() >= 16 {
                return Err(ControlError::Invalid(
                    "At most 16 Ribbon merge sources are supported".into(),
                ));
            }
            sources.push((source.to_owned(), tabs));
        }
        self.apply_merge_sources(ui, sources)
    }

    pub fn unmerge_tabs(&mut self, ui: &mut Controls, source: &str) -> Result<bool, ControlError> {
        let mut sources = self.merge_sources.clone();
        sources.retain(|entry| entry.0 != source);
        if sources.len() == self.merge_sources.len() {
            return Ok(false);
        }
        self.apply_merge_sources(ui, sources)?;
        Ok(true)
    }

    pub fn merge_source_ids(&self) -> impl Iterator<Item = &str> {
        self.merge_sources.iter().map(|entry| entry.0.as_str())
    }

    fn apply_merge_sources(
        &mut self,
        ui: &mut Controls,
        sources: Vec<(String, Vec<RibbonTab>)>,
    ) -> Result<(), ControlError> {
        let mut tabs = self.base_tabs.clone();
        for (_, fragment) in &sources {
            for tab in fragment {
                if let Some(existing) = tabs.iter_mut().find(|candidate| candidate.id == tab.id) {
                    if existing.label != tab.label
                        || existing.key_tip.to_uppercase() != tab.key_tip.to_uppercase()
                        || existing.context != tab.context
                    {
                        return Err(ControlError::Invalid(
                            "Matching merged tab IDs must share label, KeyTip and context".into(),
                        ));
                    }
                    existing.groups.extend(tab.groups.clone());
                } else {
                    tabs.push(tab.clone());
                }
            }
        }
        let group_count: usize = tabs.iter().map(|tab| tab.groups.len()).sum();
        let item_count: usize = tabs
            .iter()
            .flat_map(|tab| &tab.groups)
            .map(|group| group.items.len())
            .sum();
        if tabs.len() > 64 || group_count > 256 || item_count > 1024 {
            return Err(ControlError::Invalid(
                "Merged Ribbon exceeds 64 tabs, 256 groups or 1024 placements".into(),
            ));
        }
        let candidate = Ribbon::new(RibbonModel {
            tabs,
            quick_access: self.model.quick_access.clone(),
        })?
        .model;
        let previous = std::mem::replace(&mut self.model, candidate);
        let previous_active = self.active.clone();
        self.ensure_active();
        if let Err(error) = self.preflight(ui, self.width) {
            self.model = previous;
            self.active = previous_active;
            return Err(error);
        }
        self.merge_sources = sources;
        self.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tab(id: &str, key_tip: &str, group: &str, item_tip: &str) -> RibbonTab {
        RibbonTab {
            id: id.into(),
            label: id.into(),
            key_tip: key_tip.into(),
            context: None,
            groups: vec![RibbonGroup {
                id: group.into(),
                label: group.into(),
                collapse_priority: 0,
                items: vec![RibbonItem::new("save", RibbonItemKind::Large, item_tip)],
            }],
        }
    }
    #[test]
    fn merging_sources_unmerges_only_owned_groups_and_keeps_custom_qat() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("save", "Save");
        let mut ribbon = Ribbon::new(RibbonModel {
            tabs: vec![tab("home", "H", "base", "B")],
            quick_access: vec!["save".into()],
        })
        .unwrap();
        ribbon
            .merge_tabs(
                &mut ui,
                "document-a",
                vec![
                    tab("home", "H", "document", "D"),
                    tab("tools", "T", "tools", "S"),
                ],
            )
            .unwrap();
        ribbon
            .merge_tabs(&mut ui, "document-b", vec![tab("home", "H", "extra", "E")])
            .unwrap();
        assert_eq!(ribbon.model.tabs[0].groups.len(), 3);
        assert_eq!(
            ribbon.merge_source_ids().collect::<Vec<_>>(),
            vec!["document-a", "document-b"]
        );
        ribbon.replace_quick_access(&mut ui, vec![]).unwrap();
        assert!(ribbon.set_active_tab("tools"));
        assert!(ribbon.unmerge_tabs(&mut ui, "document-a").unwrap());
        assert_eq!(
            ribbon.model.tabs[0]
                .groups
                .iter()
                .map(|g| g.id.as_str())
                .collect::<Vec<_>>(),
            vec!["base", "extra"]
        );
        assert_eq!(ribbon.active_tab(), "home");
        assert!(ribbon.model.quick_access.is_empty());
        assert!(!ribbon.unmerge_tabs(&mut ui, "absent").unwrap());
        assert!(ui.commands_mut().drain_invocations().is_empty());
    }
    #[test]
    fn merge_conflicts_and_failed_source_replacement_leave_live_model_unchanged() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("save", "Save");
        let mut ribbon = Ribbon::new(RibbonModel {
            tabs: vec![tab("home", "H", "base", "B")],
            quick_access: vec![],
        })
        .unwrap();
        ribbon
            .merge_tabs(&mut ui, "document", vec![tab("home", "H", "document", "D")])
            .unwrap();
        let before = format!("{:?}", ribbon.model());
        for fragment in [
            tab("home", "X", "new", "N"),
            tab("home", "H", "base", "N"),
            tab("home", "H", "new", "B"),
        ] {
            assert!(
                ribbon
                    .merge_tabs(&mut ui, "document", vec![fragment])
                    .is_err()
            );
            assert_eq!(format!("{:?}", ribbon.model()), before);
            assert_eq!(
                ribbon.merge_source_ids().collect::<Vec<_>>(),
                vec!["document"]
            );
        }
        let mut unknown = tab("other", "O", "new", "N");
        unknown.groups[0].items[0].command = "missing".into();
        assert!(
            ribbon
                .merge_tabs(&mut ui, "unknown", vec![unknown])
                .is_err()
        );
        assert_eq!(format!("{:?}", ribbon.model()), before);
    }
}
