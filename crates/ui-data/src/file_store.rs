use super::*;
use crate::{DataAdapter, DataWrite, WriteCapabilities};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileAccess {
    #[default]
    ReadOnly,
    ReadWrite,
}

/// An explicit synchronous file adapter. Loads publish only complete snapshots.
/// Writes use a same-directory temporary file and atomic rename replacement.
/// A sidecar advisory lock coordinates this library's writers; arbitrary
/// external writers are not a transactional participant. Changes detected before
/// replacement are conflicts, but compare-and-rename is not an OS-level CAS.
pub struct FileDataAdapter {
    path: PathBuf,
    mapping: FileTableMapping,
    options: DataFileOptions,
    access: FileAccess,
    baseline: Vec<u8>,
    table: DataTable,
}
impl FileDataAdapter {
    pub fn open(
        path: impl AsRef<Path>,
        mapping: FileTableMapping,
        options: DataFileOptions,
        access: FileAccess,
    ) -> Result<Self, DataError> {
        mapping.validate()?;
        options.validate()?;
        let path = resolve(path.as_ref())?;
        let baseline = read_bounded(&path, options.max_file_bytes)?;
        let table = decode_data_file(&baseline, &mapping, options)?;
        Ok(Self {
            path,
            mapping,
            options,
            access,
            baseline,
            table,
        })
    }
    /// Create a missing file; an existing target is a conflict. The resulting
    /// adapter permits transactional CRUD. Parent directories are not created.
    pub fn create(
        path: impl AsRef<Path>,
        table: DataTable,
        mapping: FileTableMapping,
        options: DataFileOptions,
    ) -> Result<Self, DataError> {
        mapping.validate()?;
        options.validate()?;
        let path = resolve(path.as_ref())?;
        let bytes = encode_data_file(&table, &mapping, options)?;
        let table = DataTable::with_limits(
            table.schema().clone(),
            table.rows().to_vec(),
            options.limits,
        )?;
        replace(&path, None, &bytes, options.max_file_bytes)?;
        Ok(Self {
            path,
            mapping,
            options,
            access: FileAccess::ReadWrite,
            baseline: bytes,
            table,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn mapping(&self) -> &FileTableMapping {
        &self.mapping
    }
    pub fn options(&self) -> DataFileOptions {
        self.options
    }
    pub fn access(&self) -> FileAccess {
        self.access
    }
    /// Last successfully loaded or written table; this getter performs no I/O.
    pub fn accepted_table(&self) -> &DataTable {
        &self.table
    }
    fn transact(&mut self, operations: &[DataWrite]) -> Result<DataTable, DataError> {
        if self.access != FileAccess::ReadWrite {
            return Err(DataError::ReadOnly("file adapter".into()));
        }
        if operations.is_empty() {
            return Ok(self.table.clone());
        }
        let candidate = crate::write::apply_writes(&self.table, operations)?;
        let bytes = encode_data_file(&candidate, &self.mapping, self.options)?;
        // Prepare the returned owned snapshot before storage commit.
        let returned = candidate.clone();
        replace(
            &self.path,
            Some(&self.baseline),
            &bytes,
            self.options.max_file_bytes,
        )?;
        self.table = candidate;
        self.baseline = bytes;
        Ok(returned)
    }
}
impl DataAdapter for FileDataAdapter {
    fn load(&mut self) -> Result<DataTable, DataError> {
        let bytes = read_bounded(&self.path, self.options.max_file_bytes)?;
        let table = decode_data_file(&bytes, &self.mapping, self.options)?;
        let result = table.clone();
        self.baseline = bytes;
        self.table = table;
        Ok(result)
    }
    fn write_cell(
        &mut self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: &CellValue,
    ) -> Result<DataTable, DataError> {
        self.transact(&[DataWrite::Update {
            key,
            field: field.into(),
            expected: expected.clone(),
            value: value.clone(),
        }])
    }
    fn write_capabilities(&self) -> WriteCapabilities {
        if self.access == FileAccess::ReadWrite {
            WriteCapabilities::transactional_crud()
        } else {
            WriteCapabilities::default()
        }
    }
    fn write_batch(&mut self, operations: &[DataWrite]) -> Result<DataTable, DataError> {
        self.transact(operations)
    }
}
fn io(error: std::io::Error) -> DataError {
    // Paths and file contents are deliberately absent from errors.
    DataError::Adapter(format!(
        "file I/O: {:?} (OS {:?})",
        error.kind(),
        error.raw_os_error()
    ))
}
fn resolve(path: &Path) -> Result<PathBuf, DataError> {
    let name = path
        .file_name()
        .ok_or_else(|| DataError::Adapter("data path requires a file name".into()))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize().map_err(io)?;
    let result = parent.join(name);
    match fs::symlink_metadata(&result) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            return Err(DataError::Adapter(
                "data path must be a regular non-symlink file".into(),
            ));
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(io(error)),
        _ => {}
    }
    Ok(result)
}
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, DataError> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if metadata.file_type().is_symlink() {
        return Err(DataError::Adapter("data path cannot be a symlink".into()));
    }
    let file = File::open(path).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() {
        return Err(DataError::Adapter(
            "data path must be a regular file".into(),
        ));
    }
    if metadata.len() > limit as u64 {
        return Err(DataError::Capacity(
            "serialized file byte budget exceeded".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err(DataError::Capacity(
            "serialized file byte budget exceeded".into(),
        ));
    }
    Ok(bytes)
}
fn compare(path: &Path, expected: Option<&[u8]>, limit: usize) -> Result<(), DataError> {
    match expected {
        Some(expected) => {
            let current = read_bounded(path, limit).map_err(|_| {
                DataError::Conflict("data file changed or became unreadable".into())
            })?;
            if current != expected {
                return Err(DataError::Conflict(
                    "data file bytes changed since the accepted snapshot".into(),
                ));
            }
        }
        None => match fs::symlink_metadata(path) {
            Ok(_) => return Err(DataError::Conflict("data file already exists".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        },
    }
    Ok(())
}
struct Temp {
    path: PathBuf,
    committed: bool,
}
impl Drop for Temp {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}
fn replace(
    path: &Path,
    expected: Option<&[u8]>,
    bytes: &[u8],
    limit: usize,
) -> Result<(), DataError> {
    let mut lock_name = path.file_name().expect("resolved path").to_os_string();
    lock_name.push(".ui-data.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_file_name(lock_name))
        .map_err(io)?;
    lock.try_lock()
        .map_err(|_| DataError::Conflict("data file writer lock is busy or unavailable".into()))?;
    compare(path, expected, limit)?;
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut temporary = None;
    for _ in 0..128 {
        let token = NEXT.fetch_add(1, Ordering::Relaxed);
        let temp_path = path.with_file_name(format!(".ui-data-{}-{token}.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => {
                temporary = Some((
                    Temp {
                        path: temp_path,
                        committed: false,
                    },
                    file,
                ));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io(error)),
        }
    }
    let (mut guard, mut file) =
        temporary.ok_or_else(|| DataError::Adapter("temporary file namespace exhausted".into()))?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    drop(file);
    compare(path, expected, limit)?;
    fs::rename(&guard.path, path).map_err(io)?;
    guard.committed = true;
    // No fallible operation after the successful storage replacement.
    Ok(())
}
