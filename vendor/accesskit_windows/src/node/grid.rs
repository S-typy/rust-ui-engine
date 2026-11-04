// Local extension, Copyright 2026 rust-ui-engine contributors.
// Licensed under Apache-2.0. Upstream files retain their original licenses.
//! UIA table patterns over the current materialized AccessKit snapshot.
use super::*;
mod virtualization;
use virtualization::*;

fn container_role(node: &NodeRef<'_>) -> bool {
    matches!(node.role(), Role::Grid | Role::TreeGrid | Role::Table)
}

fn dimensions(node: &NodeRef<'_>) -> Option<(i32, i32)> {
    if !container_role(node) || node.data().is_hidden() {
        return None;
    }
    let mut parent = node.parent();
    while let Some(current) = parent {
        if current.data().is_hidden() {
            return None;
        }
        parent = current.parent();
    }
    Some((
        node.data().row_count()?.try_into().ok()?,
        node.data().column_count()?.try_into().ok()?,
    ))
}

fn containing_grid<'a>(node: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    if node.data().is_hidden() {
        return None;
    }
    let mut parent = node.parent();
    while let Some(current) = parent {
        if current.data().is_hidden() {
            return None;
        }
        if container_role(&current) {
            return dimensions(&current).map(|_| current);
        }
        parent = current.parent();
    }
    None
}

#[derive(Clone, Copy)]
struct Cell {
    row: i32,
    column: i32,
    rows: i32,
    columns: i32,
}

fn cell(node: &NodeRef<'_>) -> Option<Cell> {
    if !matches!(node.role(), Role::Cell | Role::GridCell) {
        return None;
    }
    let grid = containing_grid(node)?;
    let (rows, columns) = dimensions(&grid)?;
    let data = node.data();
    let cell = Cell {
        row: data.row_index()?.try_into().ok()?,
        column: data.column_index()?.try_into().ok()?,
        rows: data.row_span().unwrap_or(1).try_into().ok()?,
        columns: data.column_span().unwrap_or(1).try_into().ok()?,
    };
    (cell.rows > 0
        && cell.columns > 0
        && cell.row.checked_add(cell.rows)? <= rows
        && cell.column.checked_add(cell.columns)? <= columns)
        .then_some(cell)
}

/// Does not cross nested tables or hidden subtrees. Traversal is iterative so
/// a deeply nested application snapshot cannot overflow the provider stack.
fn descendants<'a>(grid: &NodeRef<'a>) -> Vec<NodeRef<'a>> {
    let mut pending: Vec<_> = grid.children().collect();
    let mut result = Vec::new();
    while let Some(node) = pending.pop() {
        if node.data().is_hidden() || container_role(&node) {
            continue;
        }
        pending.extend(node.children());
        result.push(node);
    }
    result
}

fn headers(grid: &NodeRef<'_>, column: bool, item: Option<Cell>) -> Vec<FullNodeId> {
    let Some((rows, columns)) = dimensions(grid) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for node in descendants(grid) {
        let expected = if column {
            Role::ColumnHeader
        } else {
            Role::RowHeader
        };
        if node.role() != expected {
            continue;
        }
        let data = node.data();
        let (index, span, limit) = if column {
            (
                data.column_index(),
                data.column_span().unwrap_or(1),
                columns,
            )
        } else {
            (data.row_index(), data.row_span().unwrap_or(1), rows)
        };
        let Some(index) = index else { continue };
        let Some(end) = index.checked_add(span) else {
            continue;
        };
        if span == 0 || end > limit as usize {
            continue;
        }
        if let Some(item) = item {
            let (start, count) = if column {
                (item.column, item.columns)
            } else {
                (item.row, item.rows)
            };
            if index >= (start + count) as usize || end <= start as usize {
                continue;
            }
        }
        found.push((index, node.id()));
    }
    found.sort_by_key(|(index, _)| *index);
    found.into_iter().map(|(_, id)| id).collect()
}

impl NodeWrapper<'_> {
    pub(super) fn enqueue_grid_property_changes(
        &self,
        queue: &mut Vec<QueuedEvent>,
        element: &IRawElementProviderSimple,
        old: &NodeWrapper<'_>,
    ) {
        let before = dimensions(old.node);
        let after = dimensions(self.node);
        let old_cell = cell(old.node);
        let new_cell = cell(self.node);
        for (property, previous, current) in [
            (
                UIA_IsGridPatternAvailablePropertyId,
                before.is_some(),
                after.is_some(),
            ),
            (
                UIA_IsTablePatternAvailablePropertyId,
                before.is_some(),
                after.is_some(),
            ),
            (
                UIA_IsGridItemPatternAvailablePropertyId,
                old_cell.is_some(),
                new_cell.is_some(),
            ),
            (
                UIA_IsTableItemPatternAvailablePropertyId,
                old_cell.is_some(),
                new_cell.is_some(),
            ),
        ] {
            if previous != current {
                enqueue_property_change(queue, element, property, previous.into(), current.into());
            }
        }
        for (property, previous, current) in [
            (
                UIA_GridRowCountPropertyId,
                before.map(|d| d.0),
                after.map(|d| d.0),
            ),
            (
                UIA_GridColumnCountPropertyId,
                before.map(|d| d.1),
                after.map(|d| d.1),
            ),
            (
                UIA_GridItemRowPropertyId,
                old_cell.map(|c| c.row),
                new_cell.map(|c| c.row),
            ),
            (
                UIA_GridItemColumnPropertyId,
                old_cell.map(|c| c.column),
                new_cell.map(|c| c.column),
            ),
            (
                UIA_GridItemRowSpanPropertyId,
                old_cell.map(|c| c.rows),
                new_cell.map(|c| c.rows),
            ),
            (
                UIA_GridItemColumnSpanPropertyId,
                old_cell.map(|c| c.columns),
                new_cell.map(|c| c.columns),
            ),
        ] {
            if previous != current {
                enqueue_property_change(queue, element, property, previous.into(), current.into());
            }
        }
    }
}

impl PlatformNode_Impl {
    pub(super) fn grid_pattern_provider(
        &self,
        pattern: UIA_PATTERN_ID,
    ) -> Option<Result<IUnknown>> {
        if !matches!(
            pattern,
            UIA_GridPatternId
                | UIA_GridItemPatternId
                | UIA_TablePatternId
                | UIA_TableItemPatternId
                | UIA_ItemContainerPatternId
                | UIA_VirtualizedItemPatternId
        ) {
            return None;
        }
        Some(self.resolve_with_context(|node, context| match pattern {
            UIA_ItemContainerPatternId if virtual_generation(&node).is_some() => {
                self.to_interface::<IItemContainerProvider>().cast()
            }
            UIA_VirtualizedItemPatternId if cell(&node).is_some() => {
                let grid = containing_grid(&node).ok_or_else(not_supported)?;
                let generation = virtual_generation(&grid).ok_or_else(not_supported)?;
                let c = cell(&node).ok_or_else(not_supported)?;
                VirtualCell::new(context, grid.id(), generation, c.row, c.column)
                    .into_interface::<IVirtualizedItemProvider>()
                    .cast()
            }
            UIA_GridPatternId if dimensions(&node).is_some() => {
                self.to_interface::<IGridProvider>().cast()
            }
            UIA_TablePatternId if dimensions(&node).is_some() => {
                self.to_interface::<ITableProvider>().cast()
            }
            UIA_GridItemPatternId if cell(&node).is_some() => {
                self.to_interface::<IGridItemProvider>().cast()
            }
            UIA_TableItemPatternId if cell(&node).is_some() => {
                self.to_interface::<ITableItemProvider>().cast()
            }
            _ => Err(Error::empty()),
        }))
    }

    pub(super) fn grid_property_value(&self, property: UIA_PROPERTY_ID) -> Option<Result<VARIANT>> {
        if property == UIA_IsItemContainerPatternAvailablePropertyId {
            return Some(
                self.resolve(|node| Ok(VARIANT::from(virtual_generation(&node).is_some()))),
            );
        }
        if property == UIA_IsVirtualizedItemPatternAvailablePropertyId {
            return Some(self.resolve(|node| {
                Ok(VARIANT::from(
                    cell(&node).is_some()
                        && containing_grid(&node)
                            .is_some_and(|grid| virtual_generation(&grid).is_some()),
                ))
            }));
        }
        if !matches!(
            property,
            UIA_IsGridPatternAvailablePropertyId
                | UIA_IsTablePatternAvailablePropertyId
                | UIA_IsGridItemPatternAvailablePropertyId
                | UIA_IsTableItemPatternAvailablePropertyId
                | UIA_GridRowCountPropertyId
                | UIA_GridColumnCountPropertyId
                | UIA_GridItemRowPropertyId
                | UIA_GridItemColumnPropertyId
                | UIA_GridItemRowSpanPropertyId
                | UIA_GridItemColumnSpanPropertyId
                | UIA_GridItemContainingGridPropertyId
                | UIA_TableRowOrColumnMajorPropertyId
                | UIA_TableRowHeadersPropertyId
                | UIA_TableColumnHeadersPropertyId
                | UIA_TableItemRowHeaderItemsPropertyId
                | UIA_TableItemColumnHeaderItemsPropertyId
        ) {
            return None;
        }
        let header = match property {
            UIA_TableRowHeadersPropertyId => Some((false, false)),
            UIA_TableColumnHeadersPropertyId => Some((true, false)),
            UIA_TableItemRowHeaderItemsPropertyId => Some((false, true)),
            UIA_TableItemColumnHeaderItemsPropertyId => Some((true, true)),
            _ => None,
        };
        if let Some((column, item)) = header {
            return Some(self.grid_headers(column, item).map(|array| {
                // Variant owns the SAFEARRAY returned by the provider.
                VARIANT {
                    Anonymous: VARIANT_0 {
                        Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                            vt: VT_ARRAY | VT_UNKNOWN,
                            wReserved1: 0,
                            wReserved2: 0,
                            wReserved3: 0,
                            Anonymous: VARIANT_0_0_0 { parray: array },
                        }),
                    },
                }
            }));
        }
        Some(self.resolve_with_context(|node, context| {
            let dims = dimensions(&node);
            let item = cell(&node);
            Ok(match property {
                UIA_IsGridPatternAvailablePropertyId | UIA_IsTablePatternAvailablePropertyId => {
                    dims.is_some().into()
                }
                UIA_IsGridItemPatternAvailablePropertyId
                | UIA_IsTableItemPatternAvailablePropertyId => item.is_some().into(),
                UIA_GridRowCountPropertyId => dims.ok_or_else(not_supported)?.0.into(),
                UIA_GridColumnCountPropertyId => dims.ok_or_else(not_supported)?.1.into(),
                UIA_GridItemRowPropertyId => item.ok_or_else(not_supported)?.row.into(),
                UIA_GridItemColumnPropertyId => item.ok_or_else(not_supported)?.column.into(),
                UIA_GridItemRowSpanPropertyId => item.ok_or_else(not_supported)?.rows.into(),
                UIA_GridItemColumnSpanPropertyId => item.ok_or_else(not_supported)?.columns.into(),
                UIA_GridItemContainingGridPropertyId => {
                    item.ok_or_else(not_supported)?;
                    let grid = containing_grid(&node).ok_or_else(not_supported)?;
                    let provider = context
                        .get_or_create_platform_node(grid.id())
                        .into_interface::<IRawElementProviderSimple>();
                    provider.cast::<IUnknown>()?.into()
                }
                UIA_TableRowOrColumnMajorPropertyId => {
                    dims.ok_or_else(not_supported)?;
                    RowOrColumnMajor_RowMajor.0.into()
                }
                _ => unreachable!(),
            })
        }))
    }

    fn grid_headers(&self, column: bool, for_item: bool) -> Result<*mut SAFEARRAY> {
        self.resolve_with_context(|node, context| {
            let (grid, item) = if for_item {
                (
                    containing_grid(&node).ok_or_else(not_supported)?,
                    Some(cell(&node).ok_or_else(not_supported)?),
                )
            } else {
                dimensions(&node).ok_or_else(not_supported)?;
                (node, None)
            };
            let values = headers(&grid, column, item)
                .into_iter()
                .map(|id| {
                    context
                        .get_or_create_platform_node(id)
                        .into_interface::<IRawElementProviderSimple>()
                        .cast::<IUnknown>()
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(safe_array_from_com_slice(&values))
        })
    }
}

#[allow(non_snake_case)]
impl IGridProvider_Impl for PlatformNode_Impl {
    fn RowCount(&self) -> Result<i32> {
        self.resolve(|node| dimensions(&node).map(|d| d.0).ok_or_else(not_supported))
    }
    fn ColumnCount(&self) -> Result<i32> {
        self.resolve(|node| dimensions(&node).map(|d| d.1).ok_or_else(not_supported))
    }
    fn GetItem(&self, row: i32, column: i32) -> Result<IRawElementProviderSimple> {
        let (materialized, pending) = self.resolve_with_context(|node, context| {
            let (rows, columns) = dimensions(&node).ok_or_else(not_supported)?;
            if row < 0 || column < 0 || row >= rows || column >= columns {
                return Err(invalid_arg());
            }
            let mut found = None;
            for child in descendants(&node) {
                let Some(c) = cell(&child) else { continue };
                if row >= c.row
                    && row < c.row + c.rows
                    && column >= c.column
                    && column < c.column + c.columns
                {
                    if found.is_some() {
                        return Err(invalid_operation());
                    }
                    found = Some(child.id());
                }
            }
            if let Some(id) = found {
                return Ok((
                    Some(context.get_or_create_platform_node(id).into_interface()),
                    None,
                ));
            }
            let generation = virtual_generation(&node).ok_or_else(element_not_available)?;
            let c = virtual_span(&node, row, column)?;
            Ok((
                None,
                Some(VirtualCell::new(
                    context,
                    node.id(),
                    generation,
                    c.row,
                    c.column,
                )),
            ))
        })?;
        if let Some(cell) = materialized {
            return Ok(cell);
        }
        let pending = pending.ok_or_else(element_not_available)?;
        // GetItem is an explicit logical-cell navigation request. Some native
        // readers require a full fragment here and do not realize placeholders.
        // Enumeration stays passive through ItemContainer. Dispatch only after
        // dropping the tree read lock; selection/focus/drafts are unchanged.
        let realization = pending.to_interface::<IVirtualizedItemProvider>();
        unsafe { realization.Realize()? };
        pending.materialized()
    }
}

#[allow(non_snake_case)]
impl IGridItemProvider_Impl for PlatformNode_Impl {
    fn Row(&self) -> Result<i32> {
        self.resolve(|n| cell(&n).map(|c| c.row).ok_or_else(not_supported))
    }
    fn Column(&self) -> Result<i32> {
        self.resolve(|n| cell(&n).map(|c| c.column).ok_or_else(not_supported))
    }
    fn RowSpan(&self) -> Result<i32> {
        self.resolve(|n| cell(&n).map(|c| c.rows).ok_or_else(not_supported))
    }
    fn ColumnSpan(&self) -> Result<i32> {
        self.resolve(|n| cell(&n).map(|c| c.columns).ok_or_else(not_supported))
    }
    fn ContainingGrid(&self) -> Result<IRawElementProviderSimple> {
        self.resolve_with_context(|node, context| {
            cell(&node).ok_or_else(not_supported)?;
            let grid = containing_grid(&node).ok_or_else(not_supported)?;
            Ok(context
                .get_or_create_platform_node(grid.id())
                .into_interface())
        })
    }
}

#[allow(non_snake_case)]
impl ITableProvider_Impl for PlatformNode_Impl {
    fn GetRowHeaders(&self) -> Result<*mut SAFEARRAY> {
        self.grid_headers(false, false)
    }
    fn GetColumnHeaders(&self) -> Result<*mut SAFEARRAY> {
        self.grid_headers(true, false)
    }
    fn RowOrColumnMajor(&self) -> Result<RowOrColumnMajor> {
        self.resolve(|node| {
            dimensions(&node).ok_or_else(not_supported)?;
            Ok(RowOrColumnMajor_RowMajor)
        })
    }
}

#[allow(non_snake_case)]
impl ITableItemProvider_Impl for PlatformNode_Impl {
    fn GetRowHeaderItems(&self) -> Result<*mut SAFEARRAY> {
        self.grid_headers(false, true)
    }
    fn GetColumnHeaderItems(&self) -> Result<*mut SAFEARRAY> {
        self.grid_headers(true, true)
    }
}

#[cfg(test)]
mod tests;
