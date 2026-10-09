use std::collections::{BTreeMap, HashMap, HashSet};

use crate::{GridError, RowKey, TreeDataSource};

pub(crate) type Locations = HashMap<RowKey, (RowKey, usize)>;

#[derive(Debug, Clone)]
pub(crate) struct Span {
    pub start: usize,
    pub count: usize,
    pub parent: Option<RowKey>,
    pub child_start: usize,
    pub depth: usize,
    pub placeholder: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SparseIndex {
    pub spans: Vec<Span>,
    pub count: usize,
}

pub(crate) fn location(
    source: &impl TreeDataSource,
    locations: &Locations,
    row: RowKey,
) -> Option<(Option<RowKey>, usize)> {
    if let Some(&(parent, index)) = locations.get(&row) {
        Some((Some(parent), index))
    } else {
        let parent = source.parent(row);
        let index = source.position(row)?;
        (source.child_at(parent, index) == Some(row)).then_some((parent, index))
    }
}

pub(crate) fn count(
    source: &impl TreeDataSource,
    loaded: &HashMap<RowKey, Vec<RowKey>>,
    parent: Option<RowKey>,
) -> Option<usize> {
    parent
        .and_then(|key| loaded.get(&key))
        .map(Vec::len)
        .or_else(|| source.child_count(parent))
}

pub(crate) fn child(
    source: &impl TreeDataSource,
    loaded: &HashMap<RowKey, Vec<RowKey>>,
    parent: Option<RowKey>,
    index: usize,
) -> Option<RowKey> {
    if let Some(rows) = parent.and_then(|key| loaded.get(&key)) {
        rows.get(index).copied()
    } else {
        source.child_at(parent, index)
    }
}

impl SparseIndex {
    pub fn rebuild(
        source: &impl TreeDataSource,
        expanded: &HashSet<RowKey>,
        loaded: &HashMap<RowKey, Vec<RowKey>>,
        locations: &Locations,
    ) -> Result<Self, GridError> {
        let mut groups: BTreeMap<Option<RowKey>, BTreeMap<usize, RowKey>> = BTreeMap::new();
        for &row in expanded {
            let Some((parent, position)) = location(source, locations, row) else {
                continue;
            };
            let mut current = parent;
            let mut seen = HashSet::from([row]);
            let mut visible = true;
            while let Some(ancestor) = current {
                if !seen.insert(ancestor) || seen.len() > 256 {
                    return Err(GridError::InvalidHierarchy(
                        "cycle or hierarchy depth above 256",
                    ));
                }
                if !expanded.contains(&ancestor) {
                    visible = false;
                    break;
                }
                current = location(source, locations, ancestor)
                    .ok_or(GridError::UnknownRow(ancestor))?
                    .0;
            }
            if visible {
                groups.entry(parent).or_default().insert(position, row);
            }
        }
        let mut result = Self::default();
        result.collection(source, loaded, &groups, None, 0)?;
        Ok(result)
    }

    fn push(
        &mut self,
        parent: Option<RowKey>,
        child_start: usize,
        count: usize,
        depth: usize,
        placeholder: bool,
    ) -> Result<(), GridError> {
        if count == 0 {
            return Ok(());
        }
        self.spans.push(Span {
            start: self.count,
            count,
            parent,
            child_start,
            depth,
            placeholder,
        });
        self.count = self.count.checked_add(count).ok_or(GridError::Capacity)?;
        Ok(())
    }

    fn collection(
        &mut self,
        source: &impl TreeDataSource,
        loaded: &HashMap<RowKey, Vec<RowKey>>,
        groups: &BTreeMap<Option<RowKey>, BTreeMap<usize, RowKey>>,
        parent: Option<RowKey>,
        depth: usize,
    ) -> Result<(), GridError> {
        if depth > 256 {
            return Err(GridError::InvalidHierarchy("hierarchy depth above 256"));
        }
        let Some(total) = count(source, loaded, parent) else {
            if parent.is_none() {
                return Err(GridError::InvalidHierarchy("root row count must be known"));
            }
            return self.push(parent, 0, 1, depth, true);
        };
        let mut next = 0;
        if let Some(branches) = groups.get(&parent) {
            for (&position, &row) in branches {
                if position >= total || child(source, loaded, parent, position) != Some(row) {
                    return Err(GridError::InvalidHierarchy(
                        "child positions do not match the source",
                    ));
                }
                self.push(parent, next, position + 1 - next, depth, false)?;
                self.collection(source, loaded, groups, Some(row), depth + 1)?;
                next = position + 1;
            }
        }
        self.push(parent, next, total - next, depth, false)
    }

    pub fn span_at(&self, index: usize) -> Option<&Span> {
        if index >= self.count {
            return None;
        }
        let position = self
            .spans
            .partition_point(|span| span.start + span.count <= index);
        self.spans.get(position)
    }

    pub fn index_of(&self, parent: Option<RowKey>, position: usize) -> Option<usize> {
        self.spans
            .iter()
            .find(|span| {
                !span.placeholder
                    && span.parent == parent
                    && position >= span.child_start
                    && position - span.child_start < span.count
            })
            .map(|span| span.start + position - span.child_start)
    }
}
