//! Sparse logical geometry and identity leases for viewport materialization.
//!
//! Data ordering and editor state belong to the caller. This module never
//! allocates an entry for each logical item or stores a visual/business value.
use std::{
    collections::{HashMap, HashSet},
    fmt,
    hash::Hash,
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_VIRTUAL_ITEMS: usize = 1_000_000_000;
pub const MAX_MEASURED_EXTENTS: usize = 65_536;
pub const MAX_REALIZED_ITEMS: usize = 4096;
pub const MAX_VIRTUAL_OVERSCAN: usize = 1024;
pub const MAX_VIRTUAL_EXTENT: f64 = 1_000_000_000_000.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VirtualError {
    InvalidGeometry,
    UnknownIndex,
    DuplicateIndex,
    DuplicateKey,
    Capacity,
    GenerationExhausted,
}
impl fmt::Display for VirtualError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidGeometry => "virtual geometry must be finite and within its bounds",
            Self::UnknownIndex => "virtual item index is outside the source",
            Self::DuplicateIndex => "duplicate measured item index",
            Self::DuplicateKey => "duplicate materialized item key",
            Self::Capacity => "virtualization resource limit exceeded",
            Self::GenerationExhausted => "virtualization generation exhausted",
        })
    }
}
impl std::error::Error for VirtualError {}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MeasuredExtent {
    index: usize,
    extent: f64,
    delta_sum: f64,
}

/// An estimated size for every item, with bounded sparse measured overrides.
/// Offset lookup is O(log measured); offset-to-index is O(log count * log measured).
/// Replacing measurements is atomic and O(measured log measured).
#[derive(Clone, Debug, PartialEq)]
pub struct SparseAxis {
    count: usize,
    default_extent: f64,
    measured: Vec<MeasuredExtent>,
    total: f64,
}
fn valid_extent(value: f64) -> bool {
    value.is_finite() && (1.0..=1_000_000.0).contains(&value)
}
impl SparseAxis {
    pub fn new(count: usize, default_extent: f64) -> Result<Self, VirtualError> {
        let mut axis = Self {
            count: 0,
            default_extent: 1.0,
            measured: Vec::new(),
            total: 0.0,
        };
        axis.replace(count, default_extent, std::iter::empty())?;
        Ok(axis)
    }
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn default_extent(&self) -> f64 {
        self.default_extent
    }
    pub fn total_extent(&self) -> f64 {
        self.total
    }
    pub fn override_count(&self) -> usize {
        self.measured.len()
    }
    pub fn overrides(&self) -> impl Iterator<Item = (usize, f64)> + '_ {
        self.measured
            .iter()
            .map(|value| (value.index, value.extent))
    }
    /// Extents are 1..=1,000,000 logical pixels. Aggregate extent is <=1e12.
    /// An override equal to the default is discarded, without consuming capacity.
    pub fn replace(
        &mut self,
        count: usize,
        default_extent: f64,
        overrides: impl IntoIterator<Item = (usize, f64)>,
    ) -> Result<(), VirtualError> {
        if count > MAX_VIRTUAL_ITEMS {
            return Err(VirtualError::Capacity);
        }
        if !valid_extent(default_extent) {
            return Err(VirtualError::InvalidGeometry);
        }
        let mut measured = Vec::new();
        let mut seen = HashSet::new();
        for (index, extent) in overrides {
            if index >= count {
                return Err(VirtualError::UnknownIndex);
            }
            if !valid_extent(extent) {
                return Err(VirtualError::InvalidGeometry);
            }
            if !seen.insert(index) {
                return Err(VirtualError::DuplicateIndex);
            }
            // Bound iterator processing even if all supplied extents equal default.
            if seen.len() > MAX_MEASURED_EXTENTS {
                return Err(VirtualError::Capacity);
            }
            if extent != default_extent {
                measured.push(MeasuredExtent {
                    index,
                    extent,
                    delta_sum: 0.0,
                });
            }
        }
        measured.sort_unstable_by_key(|v| v.index);
        let mut delta = 0.0;
        for value in &mut measured {
            delta += value.extent - default_extent;
            value.delta_sum = delta;
        }
        let total = count as f64 * default_extent + delta;
        if !total.is_finite() || !(0.0..=MAX_VIRTUAL_EXTENT).contains(&total) {
            return Err(VirtualError::InvalidGeometry);
        }
        *self = Self {
            count,
            default_extent,
            measured,
            total,
        };
        Ok(())
    }
    /// Set or forget one measurement. Failure leaves every cached offset intact.
    pub fn set_extent(&mut self, index: usize, extent: Option<f64>) -> Result<(), VirtualError> {
        if index >= self.count {
            return Err(VirtualError::UnknownIndex);
        }
        if extent.is_some_and(|v| !valid_extent(v)) {
            return Err(VirtualError::InvalidGeometry);
        }
        let mut values: Vec<_> = self.overrides().filter(|v| v.0 != index).collect();
        if let Some(extent) = extent.filter(|v| *v != self.default_extent) {
            values.push((index, extent));
        }
        self.replace(self.count, self.default_extent, values)
    }
    /// Offset of an item start; index == count returns total extent.
    pub fn offset_of(&self, index: usize) -> Option<f64> {
        if index > self.count {
            return None;
        }
        let before = self.measured.partition_point(|value| value.index < index);
        let delta = before
            .checked_sub(1)
            .map_or(0.0, |i| self.measured[i].delta_sum);
        Some(index as f64 * self.default_extent + delta)
    }
    pub fn extent_at(&self, index: usize) -> Option<f64> {
        if index >= self.count {
            return None;
        }
        Some(
            self.measured
                .binary_search_by_key(&index, |v| v.index)
                .map_or(self.default_extent, |i| self.measured[i].extent),
        )
    }
    /// The half-open item containing offset. Total extent itself is outside.
    pub fn index_at(&self, offset: f64) -> Option<usize> {
        if !offset.is_finite() || offset < 0.0 || offset >= self.total {
            return None;
        }
        let (mut lo, mut hi) = (0, self.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.offset_of(mid + 1).unwrap() <= offset {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        (lo < self.count).then_some(lo)
    }
    fn end_index(&self, offset: f64) -> usize {
        let (mut lo, mut hi) = (0, self.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.offset_of(mid).unwrap() < offset {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        lo
    }
    /// Clamp scroll, calculate visible items, then add overscan within the budget.
    /// A budget smaller than the visible range is an error, never a partial view.
    pub fn range(
        &self,
        scroll: f64,
        viewport: f64,
        overscan: VirtualOverscan,
        max_realized: usize,
    ) -> Result<VirtualRange, VirtualError> {
        if !scroll.is_finite() || !viewport.is_finite() || viewport < 0.0 {
            return Err(VirtualError::InvalidGeometry);
        }
        if max_realized == 0
            || max_realized > MAX_REALIZED_ITEMS
            || overscan.before > MAX_VIRTUAL_OVERSCAN
            || overscan.after > MAX_VIRTUAL_OVERSCAN
        {
            return Err(VirtualError::Capacity);
        }
        let scroll = scroll.clamp(0.0, (self.total - viewport).max(0.0));
        if viewport == 0.0 || self.count == 0 {
            let start = self.index_at(scroll).unwrap_or(self.count);
            return Ok(VirtualRange {
                visible: start..start,
                materialized: start..start,
                scroll,
                total_extent: self.total,
            });
        }
        let first = self.index_at(scroll).unwrap_or(self.count);
        let end = self.end_index((scroll + viewport).min(self.total));
        let visible = first..end;
        if visible.len() > max_realized {
            return Err(VirtualError::Capacity);
        }
        let spare = max_realized - visible.len();
        let requested_before = overscan.before.min(first);
        let requested_after = overscan.after.min(self.count - end);
        let mut before = requested_before.min(spare / 2);
        let after = requested_after.min(spare - before);
        before = requested_before.min(spare - after);
        Ok(VirtualRange {
            visible,
            materialized: first - before..end + after,
            scroll,
            total_extent: self.total,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualOverscan {
    pub before: usize,
    pub after: usize,
}
impl Default for VirtualOverscan {
    fn default() -> Self {
        Self {
            before: 2,
            after: 2,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct VirtualRange {
    pub visible: Range<usize>,
    pub materialized: Range<usize>,
    pub scroll: f64,
    pub total_extent: f64,
}

/// A source-owned stable key plus the pixel offset within its item. The caller
/// remaps measurements/order before restoring. A missing key falls back to the
/// old index (next item after a removal), then the last item, or zero when empty.
#[derive(Clone, Debug, PartialEq)]
pub struct StableScrollAnchor<K> {
    pub key: K,
    pub fallback_index: usize,
    pub within_item: f64,
}
impl<K> StableScrollAnchor<K> {
    pub fn capture(
        axis: &SparseAxis,
        scroll: f64,
        key_at: impl FnOnce(usize) -> Option<K>,
    ) -> Option<Self> {
        if !scroll.is_finite() || axis.count == 0 {
            return None;
        }
        let scroll = scroll.max(0.0);
        let index = axis.index_at(scroll).unwrap_or(axis.count - 1);
        Some(Self {
            key: key_at(index)?,
            fallback_index: index,
            within_item: (scroll - axis.offset_of(index)?).clamp(0.0, axis.extent_at(index)?),
        })
    }
    pub fn restore(&self, axis: &SparseAxis, index_of: impl FnOnce(&K) -> Option<usize>) -> f64 {
        if axis.count == 0 {
            return 0.0;
        }
        let index = index_of(&self.key)
            .filter(|i| *i < axis.count)
            .unwrap_or_else(|| self.fallback_index.min(axis.count - 1));
        let within = if self.within_item.is_finite() {
            self.within_item.clamp(0.0, axis.extent_at(index).unwrap())
        } else {
            0.0
        };
        let top = axis.offset_of(index).unwrap();
        let end = axis.offset_of(index + 1).unwrap();
        // A shrinking row must remain the anchor, rather than landing exactly
        // on the following row's half-open boundary.
        (top + within).min(f64::from_bits(end.to_bits() - 1).max(top))
    }
}

static NEXT_RECYCLER: AtomicU64 = AtomicU64::new(1);
/// Opaque identity for one assignment. Reassigning its slot invalidates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VirtualLease {
    owner: u64,
    slot: usize,
    generation: u64,
}
impl VirtualLease {
    pub fn slot(self) -> usize {
        self.slot
    }
    pub fn generation(self) -> u64 {
        self.generation
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeasedKey<K> {
    pub key: K,
    pub lease: VirtualLease,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualReconciliation<K> {
    /// In requested display order. Same-key assignments preserve identity.
    pub active: Vec<LeasedKey<K>>,
    pub released: Vec<LeasedKey<K>>,
    pub assigned: Vec<LeasedKey<K>>,
}
struct RecyclingSlot<K> {
    key: Option<K>,
    generation: u64,
}
/// Bounded identity slots, without editor or widget state. Owners must destroy
/// or reset released visuals before associating content with an assigned lease.
pub struct RecyclingSlots<K> {
    owner: u64,
    sequence: u64,
    maximum: usize,
    slots: Vec<RecyclingSlot<K>>,
}
impl<K: Clone + Eq + Hash> RecyclingSlots<K> {
    pub fn new(maximum: usize) -> Result<Self, VirtualError> {
        if maximum == 0 || maximum > MAX_REALIZED_ITEMS {
            return Err(VirtualError::Capacity);
        }
        let owner = NEXT_RECYCLER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| VirtualError::GenerationExhausted)?;
        Ok(Self {
            owner,
            sequence: 0,
            maximum,
            slots: Vec::new(),
        })
    }
    pub fn capacity(&self) -> usize {
        self.maximum
    }
    pub fn allocated_slots(&self) -> usize {
        self.slots.len()
    }
    pub fn active_count(&self) -> usize {
        self.slots.iter().filter(|s| s.key.is_some()).count()
    }
    pub fn resolve(&self, lease: VirtualLease) -> Option<&K> {
        if lease.owner != self.owner {
            return None;
        }
        self.slots
            .get(lease.slot)
            .filter(|s| s.generation == lease.generation)?
            .key
            .as_ref()
    }
    /// Read-only preflight, useful before constructing a caller-owned visual.
    pub fn can_reconcile(&self, keys: &[K]) -> Result<(), VirtualError> {
        if keys.len() > self.maximum {
            return Err(VirtualError::Capacity);
        }
        let mut unique = HashSet::with_capacity(keys.len());
        for key in keys {
            if !unique.insert(key) {
                return Err(VirtualError::DuplicateKey);
            }
        }
        let active: HashSet<_> = self.slots.iter().filter_map(|s| s.key.as_ref()).collect();
        let new = keys.iter().filter(|key| !active.contains(key)).count();
        self.sequence
            .checked_add(new as u64)
            .ok_or(VirtualError::GenerationExhausted)?;
        Ok(())
    }
    /// Invalid duplicates, capacity or generation overflow leave all leases intact.
    pub fn reconcile(&mut self, keys: &[K]) -> Result<VirtualReconciliation<K>, VirtualError> {
        self.can_reconcile(keys)?;
        let wanted: HashSet<_> = keys.iter().collect();
        let mut released = Vec::new();
        for (slot, entry) in self.slots.iter_mut().enumerate() {
            if entry.key.as_ref().is_some_and(|key| !wanted.contains(key)) {
                released.push(LeasedKey {
                    key: entry.key.take().unwrap(),
                    lease: VirtualLease {
                        owner: self.owner,
                        slot,
                        generation: entry.generation,
                    },
                });
            }
        }
        let mut by_key: HashMap<_, _> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| entry.key.clone().map(|key| (key, slot)))
            .collect();
        let mut free = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.key.is_none().then_some(i))
            .collect::<Vec<_>>()
            .into_iter();
        let mut assigned = Vec::new();
        let mut active = Vec::with_capacity(keys.len());
        for key in keys {
            let slot = if let Some(&slot) = by_key.get(key) {
                slot
            } else {
                let slot = free.next().unwrap_or(self.slots.len());
                self.sequence += 1; // checked for every assignment before mutation
                let entry = RecyclingSlot {
                    key: Some(key.clone()),
                    generation: self.sequence,
                };
                if slot == self.slots.len() {
                    self.slots.push(entry)
                } else {
                    self.slots[slot] = entry
                }
                by_key.insert(key.clone(), slot);
                assigned.push(LeasedKey {
                    key: key.clone(),
                    lease: VirtualLease {
                        owner: self.owner,
                        slot,
                        generation: self.sequence,
                    },
                });
                slot
            };
            active.push(LeasedKey {
                key: key.clone(),
                lease: VirtualLease {
                    owner: self.owner,
                    slot,
                    generation: self.slots[slot].generation,
                },
            });
        }
        Ok(VirtualReconciliation {
            active,
            released,
            assigned,
        })
    }
}

#[cfg(test)]
mod tests;
