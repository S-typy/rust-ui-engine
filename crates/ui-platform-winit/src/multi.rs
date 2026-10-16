use crate::{DesktopApp, RunOptions};
use rust_desktop_ui_core::Point;
use std::collections::HashSet;

/// Application identity, independent of operating-system and winit handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowKey(pub u64);

impl WindowKey {
    pub const MAIN: Self = Self(0);
}

/// Native placement. Existing windows retain user-controlled placement until
/// `placement_revision` changes explicitly.
#[derive(Clone, Debug)]
pub struct WindowSpec {
    pub key: WindowKey,
    pub options: RunOptions,
    /// Initial outer position in logical screen coordinates.
    pub position: Option<Point>,
    /// Change this value to apply position and size to an existing
    /// window once. Unchanged revisions preserve interactive native placement.
    /// Windows clamps requested placement to the nearest monitor's work area.
    /// Minimum size and native owner are fixed for a window's lifetime.
    pub placement_revision: u64,
    /// A native owner on Windows; it is not a child-window relationship.
    pub owner: Option<WindowKey>,
    /// Block the owner's window family while this owned window exists. Nested
    /// modal windows are supported; a modal must have an owner. This flag is
    /// fixed for its lifetime, as is owner identity.
    pub modal: bool,
    /// Whether a newly created window may activate. False avoids stealing focus.
    pub activate: bool,
}

impl WindowSpec {
    pub fn new(key: WindowKey, options: RunOptions) -> Self {
        Self {
            key,
            options,
            position: None,
            placement_revision: 0,
            owner: None,
            modal: false,
            activate: false,
        }
    }
}

/// Owns several retained views and their shared application state.
///
/// `windows` is reconciled after native events, before the host waits. An empty
/// set exits. A removed view must remain available through `window_mut` until
/// `window_closed`: the host first delivers focus loss, IME cancellation and
/// pointer leave. Creating or closing a native window never transfers its UI
/// document implicitly; the application coordinates retained panel ownership.
pub trait DesktopApplication {
    fn windows(&mut self) -> Result<Vec<WindowSpec>, String>;
    fn window(&self, key: WindowKey) -> Option<&dyn DesktopApp>;
    fn window_mut(&mut self, key: WindowKey) -> Option<&mut (dyn DesktopApp + '_)>;
    /// Update the desired set, or leave it unchanged to reject the close request.
    /// Retain removed views until `window_closed` is called.
    fn close_requested(&mut self, key: WindowKey) -> Result<(), String>;
    fn window_closed(&mut self, key: WindowKey) {
        let _ = key;
    }
}

/// Run several real GPU windows on one native event loop. The existing `run`
/// entry point and `DesktopApp` implementations remain source compatible.
pub fn run_multi(app: impl DesktopApplication + 'static) -> Result<(), String> {
    crate::multi_host::run(app)
}

pub(crate) fn validate_specs(specs: &[WindowSpec]) -> Result<(), String> {
    // One main window plus the docking model's sixteen floating windows.
    if specs.len() > 17 {
        return Err("At most 17 native windows are supported (main plus 16 floating)".into());
    }
    let mut keys = HashSet::new();
    for spec in specs {
        if !keys.insert(spec.key) {
            return Err(format!("Duplicate native window key {:?}", spec.key));
        }
        let options = &spec.options;
        if options.smoke_device_recovery && !options.smoke_test {
            return Err("Device recovery diagnostic requires smoke_test".into());
        }
        if !options.width.is_finite()
            || !options.height.is_finite()
            || options.width <= 0.0
            || options.height <= 0.0
            || options.min_size.is_some_and(|size| !size.is_valid())
            || spec
                .position
                .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(format!("Invalid native window geometry for {:?}", spec.key));
        }
    }
    if !specs.is_empty() && !keys.contains(&WindowKey::MAIN) {
        return Err("A nonempty native window set must contain WindowKey::MAIN".into());
    }
    for spec in specs {
        if spec.modal && spec.owner.is_none() {
            return Err("A modal window requires an owner".into());
        }
        if spec.modal
            && specs
                .iter()
                .any(|other| other.key != spec.key && other.modal && other.owner == spec.owner)
        {
            return Err("An owner can have only one immediate modal window".into());
        }
        let mut seen = HashSet::from([spec.key]);
        let mut owner = spec.owner;
        while let Some(key) = owner {
            if !seen.insert(key) {
                return Err("Native window ownership contains a cycle".into());
            }
            owner = specs
                .iter()
                .find(|candidate| candidate.key == key)
                .ok_or("Native window owner is absent")?
                .owner;
        }
    }
    Ok(())
}

pub(crate) fn modal_blocked(specs: &[WindowSpec]) -> HashSet<WindowKey> {
    fn descends(specs: &[WindowSpec], mut key: WindowKey, ancestor: WindowKey) -> bool {
        for _ in 0..=specs.len() {
            if key == ancestor {
                return true;
            }
            let Some(owner) = specs
                .iter()
                .find(|spec| spec.key == key)
                .and_then(|spec| spec.owner)
            else {
                return false;
            };
            key = owner;
        }
        false
    }
    let mut blocked = HashSet::new();
    for modal in specs.iter().filter(|spec| spec.modal) {
        let Some(owner) = modal.owner else { continue };
        for candidate in specs {
            if !descends(specs, candidate.key, modal.key)
                && (descends(specs, candidate.key, owner) || descends(specs, owner, candidate.key))
            {
                blocked.insert(candidate.key);
            }
        }
    }
    blocked
}

/// Resolve focus restoration before destroying any HWND. Multiple retiring
/// levels can disappear together; the direct owner may not survive the update.
pub(crate) fn modal_restore_target(
    previous: &[WindowSpec],
    wanted: &HashSet<WindowKey>,
    focused: Option<WindowKey>,
) -> Option<WindowKey> {
    let focused = focused?;
    let spec = previous.iter().find(|spec| spec.key == focused)?;
    if !spec.modal || wanted.contains(&focused) {
        return None;
    }
    let mut owner = spec.owner;
    for _ in 0..previous.len() {
        let key = owner?;
        if wanted.contains(&key) {
            return Some(key);
        }
        owner = previous.iter().find(|spec| spec.key == key)?.owner;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Size;

    #[test]
    fn placement_rejects_invalid_sets_before_native_creation() {
        let main = WindowSpec::new(WindowKey::MAIN, RunOptions::default());
        assert!(validate_specs(&[]).is_ok());
        assert!(validate_specs(std::slice::from_ref(&main)).is_ok());
        assert!(validate_specs(&[main.clone(), main.clone()]).is_err());
        let mut floating = WindowSpec::new(WindowKey(1), RunOptions::default());
        assert!(validate_specs(std::slice::from_ref(&floating)).is_err());
        floating.owner = Some(WindowKey::MAIN);
        assert!(validate_specs(&[floating.clone(), main.clone()]).is_ok());
        floating.position = Some(Point::new(f32::NAN, 0.0));
        assert!(validate_specs(&[main.clone(), floating.clone()]).is_err());
        floating.position = None;
        floating.options.min_size = Some(Size::new(-1.0, 0.0));
        assert!(validate_specs(&[main.clone(), floating.clone()]).is_err());
        floating.options = RunOptions::default();
        floating.options.width = f64::INFINITY;
        assert!(validate_specs(&[main, floating]).is_err());
    }

    #[test]
    fn ownership_cycles_missing_owners_and_window_limit_are_rejected() {
        let mut main = WindowSpec::new(WindowKey::MAIN, RunOptions::default());
        let mut floating = WindowSpec::new(WindowKey(1), RunOptions::default());
        floating.owner = Some(WindowKey(3));
        assert!(validate_specs(&[main.clone(), floating.clone()]).is_err());
        main.owner = Some(WindowKey(1));
        floating.owner = Some(WindowKey::MAIN);
        assert!(validate_specs(&[main, floating]).is_err());
        let many: Vec<_> = (0..18)
            .map(|key| WindowSpec::new(WindowKey(key), RunOptions::default()))
            .collect();
        assert!(validate_specs(&many).is_err());
    }

    #[test]
    fn nested_modality_blocks_owner_family_and_restores_it_after_close() {
        let main = WindowSpec::new(WindowKey::MAIN, RunOptions::default());
        let mut tool = WindowSpec::new(WindowKey(1), RunOptions::default());
        tool.owner = Some(main.key);
        let mut dialog = WindowSpec::new(WindowKey(2), RunOptions::default());
        dialog.owner = Some(main.key);
        dialog.modal = true;
        let mut nested = WindowSpec::new(WindowKey(3), RunOptions::default());
        nested.owner = Some(dialog.key);
        nested.modal = true;
        let unrelated = WindowSpec::new(WindowKey(4), RunOptions::default());
        let specs = vec![
            main.clone(),
            tool.clone(),
            dialog.clone(),
            nested,
            unrelated,
        ];
        assert!(validate_specs(&specs).is_ok());
        assert_eq!(
            modal_blocked(&specs),
            HashSet::from([main.key, tool.key, dialog.key])
        );
        let specs = vec![main.clone(), tool.clone(), dialog];
        assert_eq!(modal_blocked(&specs), HashSet::from([main.key, tool.key]));
        assert!(modal_blocked(&[main, tool]).is_empty());
    }

    #[test]
    fn modal_ambiguity_is_rejected_before_window_mutation() {
        let mut dialog = WindowSpec::new(WindowKey(1), RunOptions::default());
        dialog.modal = true;
        let main = WindowSpec::new(WindowKey::MAIN, RunOptions::default());
        assert!(validate_specs(&[main.clone(), dialog.clone()]).is_err());
        dialog.owner = Some(main.key);
        let mut sibling = dialog.clone();
        sibling.key = WindowKey(2);
        assert!(validate_specs(&[main, dialog, sibling]).is_err());
    }

    #[test]
    fn closing_multiple_modal_levels_restores_the_first_surviving_ancestor() {
        let main = WindowSpec::new(WindowKey::MAIN, RunOptions::default());
        let mut dialog = WindowSpec::new(WindowKey(1), RunOptions::default());
        dialog.owner = Some(main.key);
        dialog.modal = true;
        let mut nested = WindowSpec::new(WindowKey(2), RunOptions::default());
        nested.owner = Some(dialog.key);
        nested.modal = true;
        let mut tool = WindowSpec::new(WindowKey(3), RunOptions::default());
        tool.owner = Some(main.key);
        let previous = vec![main.clone(), dialog.clone(), nested.clone(), tool.clone()];
        assert_eq!(
            modal_restore_target(
                &previous,
                &HashSet::from([main.key, dialog.key]),
                Some(nested.key)
            ),
            Some(dialog.key)
        );
        assert_eq!(
            modal_restore_target(&previous, &HashSet::from([main.key]), Some(nested.key)),
            Some(main.key)
        );
        assert_eq!(
            modal_restore_target(&previous, &HashSet::new(), Some(nested.key)),
            None
        );
        assert_eq!(
            modal_restore_target(
                &previous,
                &HashSet::from([main.key, nested.key]),
                Some(nested.key)
            ),
            None
        );
        assert_eq!(
            modal_restore_target(&previous, &HashSet::from([main.key]), Some(tool.key)),
            None
        );
        assert_eq!(
            modal_restore_target(&previous, &HashSet::from([main.key]), None),
            None
        );
    }
}
