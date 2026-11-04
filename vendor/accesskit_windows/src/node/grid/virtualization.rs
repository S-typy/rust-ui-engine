// Copyright 2026 rust-ui-engine contributors. Apache-2.0.
//! Bounded virtual cell placeholders. Enumeration never dispatches actions;
//! explicit logical GetItem and VirtualizedItem.Realize may reveal one viewport.
use super::*;
use std::time::{Duration, Instant};

pub(super) fn virtual_generation(grid: &NodeRef<'_>) -> Option<usize> {
    dimensions(grid)?;
    grid.data().virtualization_generation()
}

pub(super) fn virtual_span(grid: &NodeRef<'_>, row: i32, column: i32) -> Result<Cell> {
    let (rows, columns) = dimensions(grid).ok_or_else(element_not_available)?;
    if row < 0 || column < 0 || row >= rows || column >= columns {
        return Err(invalid_arg());
    }
    let spans = grid.data().virtual_cell_spans();
    if spans.len() > 65_536 {
        return Err(invalid_operation());
    }
    let mut found = None;
    for span in spans {
        let end_row = span.row.checked_add(span.rows).ok_or_else(invalid_arg)?;
        let end_column = span
            .column
            .checked_add(span.columns)
            .ok_or_else(invalid_arg)?;
        if span.rows == 0
            || span.columns == 0
            || end_row > rows as usize
            || end_column > columns as usize
        {
            return Err(invalid_arg());
        }
        if (span.row..end_row).contains(&(row as usize))
            && (span.column..end_column).contains(&(column as usize))
        {
            if found.is_some() {
                return Err(invalid_operation());
            }
            found = Some(Cell {
                row: span.row as i32,
                column: span.column as i32,
                rows: span.rows as i32,
                columns: span.columns as i32,
            });
        }
    }
    Ok(found.unwrap_or(Cell {
        row,
        column,
        rows: 1,
        columns: 1,
    }))
}

#[implement(IRawElementProviderSimple, IVirtualizedItemProvider, IGridItemProvider)]
pub(super) struct VirtualCell {
    context: Weak<Context>,
    grid: FullNodeId,
    generation: usize,
    row: i32,
    column: i32,
}

impl VirtualCell {
    pub(super) fn new(
        context: &Arc<Context>,
        grid: FullNodeId,
        generation: usize,
        row: i32,
        column: i32,
    ) -> ComObject<Self> {
        Self {
            context: Arc::downgrade(context),
            grid,
            generation,
            row,
            column,
        }
        .into_object()
    }
    fn with_grid<T>(&self, f: impl FnOnce(NodeRef<'_>, &Arc<Context>) -> Result<T>) -> Result<T> {
        let context = upgrade(&self.context)?;
        let tree = context.read_tree();
        let grid = tree
            .state()
            .node_by_id(self.grid)
            .ok_or_else(element_not_available)?;
        if virtual_generation(&grid) != Some(self.generation) {
            return Err(element_not_available());
        }
        virtual_span(&grid, self.row, self.column)?;
        f(grid, &context)
    }
    pub(super) fn materialized(&self) -> Result<IRawElementProviderSimple> {
        self.with_grid(|grid, context| {
            let mut found = None;
            for node in descendants(&grid) {
                let Some(c) = cell(&node) else { continue };
                if (c.row..c.row + c.rows).contains(&self.row)
                    && (c.column..c.column + c.columns).contains(&self.column)
                {
                    if found.is_some() {
                        return Err(invalid_operation());
                    }
                    found = Some(node.id());
                }
            }
            Ok(context
                .get_or_create_platform_node(found.ok_or_else(element_not_available)?)
                .into_interface())
        })
    }
}

#[allow(non_snake_case)]
impl IRawElementProviderSimple_Impl for VirtualCell_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }
    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        self.with_grid(|_, _| Ok(()))?;
        match pattern {
            UIA_VirtualizedItemPatternId => self.to_interface::<IVirtualizedItemProvider>().cast(),
            UIA_GridItemPatternId => self.to_interface::<IGridItemProvider>().cast(),
            _ => match self.materialized() {
                Ok(cell) => unsafe { cell.GetPatternProvider(pattern) },
                Err(_) => Err(Error::empty()),
            },
        }
    }
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        self.with_grid(|_, _| Ok(()))?;
        if property == UIA_IsVirtualizedItemPatternAvailablePropertyId
            || property == UIA_IsGridItemPatternAvailablePropertyId
        {
            return Ok(VARIANT::from(true));
        }
        // These are known even before the cell's content is materialized.
        // In particular this virtual object never owns a native HWND.
        if property == UIA_NativeWindowHandlePropertyId {
            return Ok(VARIANT::from(0_i32));
        }
        if property == UIA_ControlTypePropertyId {
            return Ok(VARIANT::from(UIA_DataItemControlTypeId.0));
        }
        if property == UIA_IsControlElementPropertyId || property == UIA_IsContentElementPropertyId
        {
            return Ok(VARIANT::from(true));
        }
        if property == UIA_IsOffscreenPropertyId && self.materialized().is_err() {
            return Ok(VARIANT::from(true));
        }
        unsafe { self.materialized()?.GetPropertyValue(property) }
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        Err(Error::empty())
    }
}

#[allow(non_snake_case)]
impl IGridItemProvider_Impl for VirtualCell_Impl {
    fn Row(&self) -> Result<i32> {
        self.with_grid(|grid, _| Ok(virtual_span(&grid, self.row, self.column)?.row))
    }
    fn Column(&self) -> Result<i32> {
        self.with_grid(|grid, _| Ok(virtual_span(&grid, self.row, self.column)?.column))
    }
    fn RowSpan(&self) -> Result<i32> {
        self.with_grid(|grid, _| Ok(virtual_span(&grid, self.row, self.column)?.rows))
    }
    fn ColumnSpan(&self) -> Result<i32> {
        self.with_grid(|grid, _| Ok(virtual_span(&grid, self.row, self.column)?.columns))
    }
    fn ContainingGrid(&self) -> Result<IRawElementProviderSimple> {
        self.with_grid(|grid, context| {
            Ok(context
                .get_or_create_platform_node(grid.id())
                .into_interface())
        })
    }
}

#[allow(non_snake_case)]
impl IVirtualizedItemProvider_Impl for VirtualCell_Impl {
    fn Realize(&self) -> Result<()> {
        if self.materialized().is_ok() {
            return Ok(());
        }
        let context = upgrade(&self.context)?;
        let request = {
            let tree = context.read_tree();
            let grid = tree
                .state()
                .node_by_id(self.grid)
                .ok_or_else(element_not_available)?;
            if virtual_generation(&grid) != Some(self.generation) {
                return Err(element_not_available());
            }
            if grid.is_disabled() || !grid.data().supports_action(Action::RealizeCell) {
                return Err(element_not_enabled());
            }
            virtual_span(&grid, self.row, self.column)?;
            let (target_node, target_tree) = tree
                .state()
                .locate_node(self.grid)
                .ok_or_else(element_not_available)?;
            ActionRequest {
                action: Action::RealizeCell,
                target_node,
                target_tree,
                data: Some(ActionData::RealizeCell {
                    row: self.row as usize,
                    column: self.column as usize,
                    generation: self.generation,
                }),
            }
        };
        // Never hold a tree lock while dispatching or waiting for the UI queue.
        context.do_action(request);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if self.materialized().is_ok() {
                return Ok(());
            }
            self.with_grid(|_, _| Ok(()))?;
            if Instant::now() >= deadline {
                return Err(element_not_available());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[allow(non_snake_case)]
impl IItemContainerProvider_Impl for PlatformNode_Impl {
    fn FindItemByProperty(
        &self,
        start: windows::core::Ref<IRawElementProviderSimple>,
        property: UIA_PROPERTY_ID,
        value: &VARIANT,
    ) -> Result<IRawElementProviderSimple> {
        if property.0 != 0 || !value.is_empty() {
            return Err(invalid_arg());
        }
        // Resolve the caller-supplied COM object before acquiring our tree lock.
        let start = if let Some(start) = start.as_ref() {
            let item: IGridItemProvider =
                unsafe { start.GetPatternProvider(UIA_GridItemPatternId)? }.cast()?;
            Some(unsafe { (item.ContainingGrid()?, item.Row()?, item.Column()?) })
        } else {
            None
        };
        self.resolve_with_context(|grid, context| {
            let generation = virtual_generation(&grid).ok_or_else(not_supported)?;
            let (rows, columns) = dimensions(&grid).ok_or_else(not_supported)?;
            let count = i64::from(rows) * i64::from(columns);
            let mut ordinal = if let Some((owner, row, column)) = start {
                let expected: IRawElementProviderSimple = context
                    .get_or_create_platform_node(grid.id())
                    .into_interface();
                if owner != expected {
                    return Err(invalid_arg());
                }
                virtual_span(&grid, row, column)?;
                i64::from(row) * i64::from(columns) + i64::from(column) + 1
            } else {
                0
            };
            while ordinal < count {
                let row = (ordinal / i64::from(columns)) as i32;
                let column = (ordinal % i64::from(columns)) as i32;
                let c = virtual_span(&grid, row, column)?;
                if c.row == row && c.column == column {
                    return Ok(
                        VirtualCell::new(context, grid.id(), generation, row, column)
                            .into_interface(),
                    );
                }
                ordinal = if c.column == 0 && c.columns == columns {
                    i64::from(c.row + c.rows) * i64::from(columns)
                } else {
                    i64::from(row) * i64::from(columns) + i64::from(c.column + c.columns)
                };
            }
            Err(Error::empty())
        })
    }
}
