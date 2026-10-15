//! Versioned placement-only JSON. File access belongs to the application.
use super::model::*;
use rust_desktop_ui_core::{Axis, Rect};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// Maximum UTF-8 JSON document size, including whitespace and its final newline.
pub const MAX_DOCK_SNAPSHOT_BYTES: usize = 1_048_576;
const FORMAT: &str = "rust-ui-engine.docking";

impl DockLayout {
    /// Encode the current placement as versioned UTF-8 JSON, bounded to one MiB.
    ///
    /// Pane IDs, tab order, selected tabs, splits, floating bounds and closed
    /// panes are preserved. Document contents, widget IDs, titles, commands and
    /// the pane catalog are deliberately absent. This method performs no I/O.
    pub fn snapshot_json(&self) -> Result<String, DockError> {
        let data = SnapshotData::from_snapshot(&self.snapshot());
        let mut output = BoundedOutput(Vec::new());
        serde_json::to_writer_pretty(&mut output, &data).map_err(json_error)?;
        output.write_all(b"\n").map_err(json_error)?;
        String::from_utf8(output.0).map_err(json_error)
    }

    /// Restore placement from a strict versioned JSON document using the
    /// existing pane catalog. An error leaves placement and ID allocation intact.
    ///
    /// Unknown or duplicate fields, missing fields, unsupported versions,
    /// unknown/multiply placed pane IDs, invalid geometry and exceeded model
    /// limits are rejected. Returns whether the placement changed. No controls
    /// are created and no documents or filesystem paths are read or written.
    pub fn restore_json(&mut self, source: &str) -> Result<bool, DockError> {
        if source.len() > MAX_DOCK_SNAPSHOT_BYTES {
            return Err(invalid("Docking JSON exceeds the one-MiB UTF-8 limit"));
        }
        // Keep serde_json's recursion protection enabled. Conversion below also
        // enforces the smaller domain depth before constructing a model tree.
        let data: SnapshotData = serde_json::from_str(source).map_err(json_error)?;
        self.restore_snapshot(data.into_snapshot()?)
    }
}

fn invalid(message: &str) -> DockError {
    DockError(message.into())
}

fn json_error(error: impl std::fmt::Display) -> DockError {
    DockError(format!("Invalid docking JSON: {error}"))
}

struct BoundedOutput(Vec<u8>);

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_DOCK_SNAPSHOT_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Docking JSON exceeds the one-MiB UTF-8 limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotData {
    format: String,
    version: u32,
    // `Option` normally accepts a missing field. A custom deserialize path
    // makes this field required while still permitting an explicit null root.
    #[serde(deserialize_with = "Option::deserialize")]
    root: Option<NodeData>,
    floating: Vec<FloatingData>,
    closed: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum NodeData {
    Tabs {
        id: u64,
        panes: Vec<String>,
        active: String,
    },
    Split {
        id: u64,
        axis: AxisData,
        ratio: f32,
        first: Box<NodeData>,
        second: Box<NodeData>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AxisData {
    Horizontal,
    Vertical,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FloatingData {
    id: u64,
    bounds: BoundsData,
    root: NodeData,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundsData {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl SnapshotData {
    fn from_snapshot(snapshot: &DockSnapshot) -> Self {
        Self {
            format: FORMAT.into(),
            version: snapshot.version,
            root: snapshot.root.as_ref().map(NodeData::from_node),
            floating: snapshot
                .floating
                .iter()
                .map(|window| FloatingData {
                    id: window.id.0,
                    bounds: BoundsData {
                        x: window.bounds.x,
                        y: window.bounds.y,
                        width: window.bounds.width,
                        height: window.bounds.height,
                    },
                    root: NodeData::from_node(&window.root),
                })
                .collect(),
            closed: snapshot
                .closed
                .iter()
                .map(|id| id.as_str().into())
                .collect(),
        }
    }

    fn into_snapshot(self) -> Result<DockSnapshot, DockError> {
        if self.format != FORMAT {
            return Err(invalid("Unsupported docking JSON format"));
        }
        if self.version != DOCK_SNAPSHOT_VERSION {
            return Err(invalid("Unsupported docking JSON version"));
        }
        if self.floating.len() > MAX_DOCK_FLOATING_WINDOWS || self.closed.len() > MAX_DOCK_PANES {
            return Err(invalid("Docking JSON exceeds its container limits"));
        }
        let mut nodes = 0;
        let root = self
            .root
            .map(|node| node.into_node(1, &mut nodes))
            .transpose()?;
        let floating = self
            .floating
            .into_iter()
            .map(|window| {
                Ok(DockFloating {
                    id: DockWindowId(window.id),
                    bounds: Rect::new(
                        window.bounds.x,
                        window.bounds.y,
                        window.bounds.width,
                        window.bounds.height,
                    ),
                    root: window.root.into_node(1, &mut nodes)?,
                })
            })
            .collect::<Result<_, DockError>>()?;
        let closed = self
            .closed
            .into_iter()
            .map(DockPaneId::new)
            .collect::<Result<_, _>>()?;
        Ok(DockSnapshot {
            version: self.version,
            root,
            floating,
            closed,
        })
    }
}

impl NodeData {
    fn from_node(node: &DockNode) -> Self {
        match node {
            DockNode::Tabs { id, panes, active } => Self::Tabs {
                id: id.0,
                panes: panes.iter().map(|id| id.as_str().into()).collect(),
                active: active.as_str().into(),
            },
            DockNode::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => Self::Split {
                id: id.0,
                axis: match axis {
                    Axis::Horizontal => AxisData::Horizontal,
                    Axis::Vertical => AxisData::Vertical,
                },
                ratio: *ratio,
                first: Box::new(Self::from_node(first)),
                second: Box::new(Self::from_node(second)),
            },
        }
    }

    fn into_node(self, depth: usize, nodes: &mut usize) -> Result<DockNode, DockError> {
        *nodes += 1;
        if depth > MAX_DOCK_DEPTH || *nodes > 2 * MAX_DOCK_PANES {
            return Err(invalid("Docking JSON exceeds its tree size or depth limit"));
        }
        match self {
            Self::Tabs { id, panes, active } => {
                if panes.is_empty() || panes.len() > MAX_DOCK_PANES {
                    return Err(invalid("Invalid docking JSON tab group size"));
                }
                Ok(DockNode::Tabs {
                    id: DockGroupId(id),
                    panes: panes
                        .into_iter()
                        .map(DockPaneId::new)
                        .collect::<Result<_, _>>()?,
                    active: DockPaneId::new(active)?,
                })
            }
            Self::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => Ok(DockNode::Split {
                id: DockSplitId(id),
                axis: match axis {
                    AxisData::Horizontal => Axis::Horizontal,
                    AxisData::Vertical => Axis::Vertical,
                },
                ratio,
                first: Box::new(first.into_node(depth + 1, nodes)?),
                second: Box::new(second.into_node(depth + 1, nodes)?),
            }),
        }
    }
}

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod tests;
