use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ARENA: AtomicU64 = AtomicU64::new(1);

/// Stable identity of a retained node, scoped to the tree that created it.
///
/// IDs are deliberately opaque: removing a node invalidates every copy of its
/// ID, even if the underlying storage slot is subsequently reused. IDs from
/// distinct trees never refer to one another's nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId {
    arena: u64,
    slot: usize,
    generation: u64,
}

struct Slot<T> {
    generation: u64,
    value: Option<T>,
}

pub(crate) struct Arena<T> {
    identity: u64,
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
    len: usize,
}

impl<T> Arena<T> {
    pub(crate) fn new() -> Self {
        // Refuse to create a tree rather than wrap and alias an existing tree.
        // This process-wide limit requires creating 2^64 - 1 arenas to reach.
        let identity = NEXT_ARENA
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("the process has exhausted arena identities");
        Self {
            identity,
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    pub(crate) fn insert(&mut self, value: T) -> WidgetId {
        let slot = match self.free.pop() {
            Some(index) => {
                self.slots[index].value = Some(value);
                index
            }
            None => {
                self.slots.push(Slot {
                    generation: 0,
                    value: Some(value),
                });
                self.slots.len() - 1
            }
        };
        self.len += 1;
        WidgetId {
            arena: self.identity,
            slot,
            generation: self.slots[slot].generation,
        }
    }

    pub(crate) fn get(&self, id: WidgetId) -> Option<&T> {
        if id.arena != self.identity {
            return None;
        }
        let slot = self.slots.get(id.slot)?;
        (slot.generation == id.generation)
            .then_some(slot.value.as_ref())
            .flatten()
    }

    pub(crate) fn get_mut(&mut self, id: WidgetId) -> Option<&mut T> {
        if id.arena != self.identity {
            return None;
        }
        let slot = self.slots.get_mut(id.slot)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_mut()
    }

    pub(crate) fn remove(&mut self, id: WidgetId) -> Option<T> {
        if id.arena != self.identity {
            return None;
        }
        let slot = self.slots.get_mut(id.slot)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        self.len -= 1;
        if let Some(generation) = slot.generation.checked_add(1) {
            slot.generation = generation;
            self.free.push(id.slot);
        }
        // A generation at u64::MAX retires its slot permanently. Wrapping
        // would make an ancient ID valid again, so this slot is never reused.
        Some(value)
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reused_slot_never_revives_a_removed_id() {
        let mut arena = Arena::new();
        let first = arena.insert("first");
        assert_eq!(arena.remove(first), Some("first"));
        let second = arena.insert("second");
        assert_eq!(first.slot, second.slot);
        assert_ne!(first, second);
        assert_eq!(arena.get(first), None);
        assert_eq!(arena.get_mut(first), None);
        assert_eq!(arena.remove(first), None);
        assert_eq!(arena.get(second), Some(&"second"));
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn foreign_ids_cannot_read_mutate_or_remove_nodes() {
        let mut first = Arena::new();
        let mut second = Arena::new();
        let foreign = first.insert(1);
        let local = second.insert(2);
        assert_eq!(foreign.slot, local.slot);
        assert_eq!(foreign.generation, local.generation);
        assert_ne!(foreign, local);
        assert_eq!(second.get(foreign), None);
        assert_eq!(second.get_mut(foreign), None);
        assert_eq!(second.remove(foreign), None);
        assert_eq!(second.get(local), Some(&2));
        assert_eq!(second.len(), 1);
    }

    #[test]
    fn generation_overflow_retires_storage() {
        let mut arena = Arena::new();
        let initial = arena.insert(1);
        arena.slots[initial.slot].generation = u64::MAX;
        let final_id = WidgetId {
            generation: u64::MAX,
            ..initial
        };
        assert_eq!(arena.remove(final_id), Some(1));
        assert!(arena.free.is_empty());
        let replacement = arena.insert(2);
        assert_ne!(replacement.slot, final_id.slot);
        assert_eq!(arena.get(initial), None);
        assert_eq!(arena.get(final_id), None);
        assert_eq!(arena.get(replacement), Some(&2));
    }

    #[test]
    fn removal_drops_each_value_once() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct CountDrop(Rc<Cell<usize>>);
        impl Drop for CountDrop {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let count = Rc::new(Cell::new(0));
        let mut arena = Arena::new();
        let id = arena.insert(CountDrop(count.clone()));
        drop(arena.remove(id));
        assert_eq!(count.get(), 1);
        assert!(arena.remove(id).is_none());
        drop(arena);
        assert_eq!(count.get(), 1);
    }
}
