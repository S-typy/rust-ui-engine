use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandHistoryError {
    InvalidCapacity,
    InvalidLabel,
    Empty,
    Conflict,
    Aborted,
    Rejected(String),
}
impl std::fmt::Display for CommandHistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCapacity => write!(f, "history capacity must be in 1..=4096"),
            Self::InvalidLabel => write!(f, "history label must contain 1..=4096 bytes"),
            Self::Empty => write!(f, "history has no matching entry"),
            Self::Conflict => write!(f, "document changed outside this history"),
            Self::Aborted => write!(f, "command group has been aborted"),
            Self::Rejected(reason) => f.write_str(reason),
        }
    }
}
impl std::error::Error for CommandHistoryError {}
struct Entry<S> {
    label: String,
    before: S,
    after: S,
}

/// Bounded business-action history over a document-owned value snapshot.
///
/// `S::clone` must isolate all mutable participants. Use owned values, not shared
/// mutable handles. A group changes only a draft until commit, so failure in one
/// participant leaves every live participant unchanged. Text editor undo remains
/// separate. No callback is replayed by undo/redo.
pub struct CommandHistory<S> {
    capacity: usize,
    undo: VecDeque<Entry<S>>,
    redo: Vec<Entry<S>>,
}
impl<S> Default for CommandHistory<S> {
    fn default() -> Self {
        Self {
            capacity: 128,
            undo: VecDeque::new(),
            redo: Vec::new(),
        }
    }
}
impl<S: Clone + PartialEq> CommandHistory<S> {
    pub fn with_capacity(capacity: usize) -> Result<Self, CommandHistoryError> {
        if !(1..=4096).contains(&capacity) {
            return Err(CommandHistoryError::InvalidCapacity);
        }
        Ok(Self {
            capacity,
            undo: VecDeque::new(),
            redo: Vec::new(),
        })
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.back().map(|entry| entry.label.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|entry| entry.label.as_str())
    }
    pub fn can_undo(&self, state: &S) -> bool {
        self.undo.back().is_some_and(|entry| entry.after == *state)
    }
    pub fn can_redo(&self, state: &S) -> bool {
        self.redo.last().is_some_and(|entry| entry.before == *state)
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    /// Begin explicit grouping. Dropping the group aborts its draft. Nested groups
    /// and concurrent mutation of this state are excluded by the mutable borrows.
    pub fn begin<'a>(
        &'a mut self,
        state: &'a mut S,
        label: impl Into<String>,
    ) -> Result<CommandGroup<'a, S>, CommandHistoryError> {
        let label = label.into();
        if label.is_empty() || label.len() > 4096 {
            return Err(CommandHistoryError::InvalidLabel);
        }
        let draft = state.clone();
        Ok(CommandGroup {
            history: self,
            state,
            draft,
            label,
            aborted: false,
        })
    }
    pub fn execute(
        &mut self,
        state: &mut S,
        label: impl Into<String>,
        action: impl FnOnce(&mut S) -> Result<(), String>,
    ) -> Result<bool, CommandHistoryError> {
        let mut group = self.begin(state, label)?;
        group.apply(action)?;
        group.commit()
    }
    pub fn undo(&mut self, state: &mut S) -> Result<(), CommandHistoryError> {
        self.undo_checked(state, |_| Ok(()))
    }
    pub fn redo(&mut self, state: &mut S) -> Result<(), CommandHistoryError> {
        self.redo_checked(state, |_| Ok(()))
    }
    /// Validate a candidate against current domain constraints before replacing
    /// live state. Rejection preserves both state and history cursor.
    pub fn undo_checked(
        &mut self,
        state: &mut S,
        validate: impl FnOnce(&S) -> Result<(), String>,
    ) -> Result<(), CommandHistoryError> {
        let entry = self.undo.back().ok_or(CommandHistoryError::Empty)?;
        if entry.after != *state {
            return Err(CommandHistoryError::Conflict);
        }
        validate(&entry.before).map_err(CommandHistoryError::Rejected)?;
        let before = entry.before.clone();
        let entry = self.undo.pop_back().expect("checked entry");
        *state = before;
        self.redo.push(entry);
        Ok(())
    }
    pub fn redo_checked(
        &mut self,
        state: &mut S,
        validate: impl FnOnce(&S) -> Result<(), String>,
    ) -> Result<(), CommandHistoryError> {
        let entry = self.redo.last().ok_or(CommandHistoryError::Empty)?;
        if entry.before != *state {
            return Err(CommandHistoryError::Conflict);
        }
        validate(&entry.after).map_err(CommandHistoryError::Rejected)?;
        let after = entry.after.clone();
        let entry = self.redo.pop().expect("checked entry");
        *state = after;
        self.undo.push_back(entry);
        Ok(())
    }
}

/// An isolated, explicit business transaction spanning the participants in `S`.
pub struct CommandGroup<'a, S> {
    history: &'a mut CommandHistory<S>,
    state: &'a mut S,
    draft: S,
    label: String,
    aborted: bool,
}
impl<S: Clone + PartialEq> CommandGroup<'_, S> {
    pub fn draft(&self) -> &S {
        &self.draft
    }
    /// Any failed participant permanently aborts the group. Actions must only
    /// mutate the supplied draft; external I/O cannot be undone by snapshots.
    pub fn apply(
        &mut self,
        action: impl FnOnce(&mut S) -> Result<(), String>,
    ) -> Result<(), CommandHistoryError> {
        if self.aborted {
            return Err(CommandHistoryError::Aborted);
        }
        // Set before entering application code: even a caught panic prevents a
        // later accidental commit of a partly changed draft.
        self.aborted = true;
        action(&mut self.draft).map_err(CommandHistoryError::Rejected)?;
        self.aborted = false;
        Ok(())
    }
    /// Publish once. A no-op preserves the redo branch and returns `false`.
    pub fn commit(self) -> Result<bool, CommandHistoryError> {
        if self.aborted {
            return Err(CommandHistoryError::Aborted);
        }
        if self.draft == *self.state {
            return Ok(false);
        }
        let before = self.state.clone();
        let after = self.draft.clone();
        self.history.redo.clear();
        if self.history.undo.len() == self.history.capacity {
            self.history.undo.pop_front();
        }
        self.history.undo.push_back(Entry {
            label: self.label,
            before,
            after,
        });
        *self.state = self.draft;
        Ok(true)
    }
}
