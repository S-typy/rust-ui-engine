//! Explicit layout commands. Editor contents never enter this file.
use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_BYTES: u64 = MAX_DOCK_SNAPSHOT_BYTES as u64;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn default_path() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("LOCALAPPDATA is unavailable; use --layout-file PATH")?;
    #[cfg(not(windows))]
    let directory = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|value| PathBuf::from(value).join(".local/state")))
        .ok_or("State directory is unavailable; use --layout-file PATH")?;
    Ok(directory.join("rust-ui-engine").join("docking-layout.json"))
}

fn load(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(err)?;
    if !file.metadata().map_err(err)?.is_file() {
        return Err("Layout path must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Layout file exceeds 1 MiB".into());
    }
    String::from_utf8(bytes).map_err(err)
}

fn save(path: &Path, text: &str) -> Result<(), String> {
    if text.len() as u64 > MAX_BYTES {
        return Err("Layout file exceeds 1 MiB".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().ok_or("Layout path requires a filename")?;
    fs::create_dir_all(parent).map_err(err)?;
    // Exclusive creation and replacement in the same directory: a failed write
    // leaves the previous file intact, including on Windows.
    let mut temporary_name = name.to_os_string();
    temporary_name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = parent.join(temporary_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(err)?;
    let result = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(err)
}

impl Workspace {
    pub(super) fn layout_command(&mut self, restore: bool) -> Result<(), String> {
        let result = (|| {
            let path = self
                .layout_file
                .as_ref()
                .ok_or("Layout file is not configured")?;
            if restore {
                let text = load(path)?;
                self.layout.restore_json(&text).map_err(err)?;
                // Reapply requested positions once, even if the saved model
                // equals the current model but a native window was moved.
                self.placement_revision = self.placement_revision.wrapping_add(1);
                self.drag = None;
                self.drop_targets.clear();
                self.reconcile_focus()?;
            } else {
                let text = self.layout.snapshot_json().map_err(err)?;
                save(path, &text)?;
            }
            Ok::<_, String>(())
        })();
        self.layout_status = match result {
            Ok(()) if restore => "Расположение восстановлено.".into(),
            Ok(()) => "Расположение сохранено.".into(),
            Err(error) => {
                eprintln!(
                    "Layout {} failed: {error}",
                    if restore { "restore" } else { "save" }
                );
                format!(
                    "Не удалось {} расположение: {error}",
                    if restore {
                        "восстановить"
                    } else {
                        "сохранить"
                    }
                )
            }
        };
        if self.diagnostics {
            println!("dock-layout {}", self.layout_status);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "rust-ui-docking-{}-{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn replacement_is_complete_and_failed_save_keeps_previous_file() {
        let directory = Directory::new();
        let path = directory.0.join("layout.json");
        save(&path, "first").unwrap();
        save(&path, "replacement").unwrap();
        assert_eq!(load(&path).unwrap(), "replacement");
        assert!(save(&path, &"x".repeat(MAX_BYTES as usize + 1)).is_err());
        assert_eq!(load(&path).unwrap(), "replacement");
        // A directory cannot be replaced by a layout file; no temp is leaked.
        let blocked = directory.0.join("directory.json");
        fs::create_dir(&blocked).unwrap();
        assert!(save(&blocked, "value").is_err());
        assert!(blocked.is_dir());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    }

    #[test]
    fn explicit_save_restore_keeps_documents_and_rejects_invalid_file_atomically() {
        let directory = Directory::new();
        let path = directory.0.join("layout.json");
        let mut app = super::super::tests::application(Some("Properties"));
        app.set_layout_file(path.clone());
        assert!(!path.exists(), "construction must not write or restore");
        let mut state = app.state.borrow_mut();
        let pane = DockPaneId::new("Properties").unwrap();
        let editor = state.panes[&pane].editor();
        state
            .panes
            .get_mut(&pane)
            .unwrap()
            .ui
            .set_text(editor, "Private editor text")
            .unwrap();
        let expected = state.layout.snapshot();
        state.command(DockWindowId::MAIN, "DockSaveLayout").unwrap();
        assert!(!load(&path).unwrap().contains("Private editor text"));
        state.command(DockWindowId::MAIN, "DockReset").unwrap();
        let before_revision = state.placement_revision;
        state.command(DockWindowId::MAIN, "DockLoadLayout").unwrap();
        assert_eq!(state.layout.snapshot(), expected);
        assert_ne!(state.placement_revision, before_revision);
        assert_eq!(state.panes[&pane].editor(), editor);
        assert_eq!(state.panes[&pane].text(), "Private editor text");
        save(&path, "{bad file").unwrap();
        let revision = state.placement_revision;
        state.command(DockWindowId::MAIN, "DockLoadLayout").unwrap();
        assert_eq!(state.layout.snapshot(), expected);
        assert_eq!(state.placement_revision, revision);
        assert!(state.layout_status.starts_with("Не удалось"));
        drop(state);
        app.close_requested(WindowKey::MAIN).unwrap();
        assert_eq!(load(&path).unwrap(), "{bad file", "close must not save");
    }

    #[test]
    fn restored_bounds_ignore_queued_native_geometry_until_placement_acknowledged() {
        let directory = Directory::new();
        let mut app = super::super::tests::application(Some("Properties"));
        app.set_layout_file(directory.0.join("layout.json"));
        let specs = app.windows().unwrap();
        let key = specs
            .iter()
            .find(|spec| spec.key != WindowKey::MAIN)
            .unwrap()
            .key;
        app.state
            .borrow_mut()
            .command(DockWindowId::MAIN, "DockSaveLayout")
            .unwrap();
        let expected = app.state.borrow().layout.floating()[0].bounds;
        app.window_mut(key).unwrap().window_geometry_changed(
            Point::new(700.0, 800.0),
            Size::new(500.0, 300.0),
            1.5,
        );
        app.state
            .borrow_mut()
            .command(DockWindowId::MAIN, "DockLoadLayout")
            .unwrap();
        let revision = app.state.borrow().placement_revision;
        app.window_mut(key).unwrap().window_geometry_changed(
            Point::new(700.0, 800.0),
            Size::new(500.0, 300.0),
            1.5,
        );
        assert_eq!(app.state.borrow().layout.floating()[0].bounds, expected);
        let spec = app
            .windows()
            .unwrap()
            .into_iter()
            .find(|spec| spec.key == key)
            .unwrap();
        assert_eq!(spec.position, Some(Point::new(expected.x, expected.y)));
        assert_eq!(spec.placement_revision, revision);
        app.window_mut(key)
            .unwrap()
            .window_placement_applied(revision);
        let actual = Rect::new(40.0, 50.0, 600.0, 400.0);
        app.window_mut(key).unwrap().window_geometry_changed(
            Point::new(actual.x, actual.y),
            Size::new(actual.width, actual.height),
            1.5,
        );
        assert_eq!(app.state.borrow().layout.floating()[0].bounds, actual);
    }

    #[test]
    fn files_must_be_bounded_utf8_and_failed_read_does_not_change_layout() {
        let directory = Directory::new();
        let path = directory.0.join("layout.json");
        let mut app = super::super::tests::application(None);
        app.set_layout_file(path.clone());
        let before = app.state.borrow().layout.snapshot();
        for bytes in [vec![0xff], vec![b' '; MAX_BYTES as usize + 1]] {
            fs::write(&path, bytes).unwrap();
            app.state
                .borrow_mut()
                .command(DockWindowId::MAIN, "DockLoadLayout")
                .unwrap();
            assert_eq!(app.state.borrow().layout.snapshot(), before);
        }
        fs::remove_file(&path).unwrap();
        app.state
            .borrow_mut()
            .command(DockWindowId::MAIN, "DockLoadLayout")
            .unwrap();
        assert!(!path.exists());
        assert_eq!(app.state.borrow().layout.snapshot(), before);
    }
}
