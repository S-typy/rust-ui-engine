use super::*;

#[test]
fn sparse_million_items_allocate_only_measured_extents_and_exact_visible_rows() {
    let mut axis = SparseAxis::new(1_000_000, 32.0).unwrap();
    assert_eq!(axis.override_count(), 0);
    assert_eq!(axis.total_extent(), 32_000_000.0);
    axis.set_extent(10, Some(64.0)).unwrap();
    axis.set_extent(900_000, Some(16.0)).unwrap();
    assert_eq!(axis.override_count(), 2);
    let offset = axis.offset_of(500_000).unwrap();
    assert_eq!(offset, 16_000_032.0);
    let view = axis
        .range(offset, 320.0, VirtualOverscan::default(), 64)
        .unwrap();
    assert_eq!(view.visible, 500_000..500_010);
    assert_eq!(view.materialized, 499_998..500_012);
    assert_eq!(view.materialized.len(), 14);
    assert_eq!(axis.override_count(), 2);
    axis.set_extent(10, None).unwrap();
    assert_eq!(axis.offset_of(500_000), Some(16_000_000.0));
}

#[test]
fn sparse_axis_matches_dense_reference_for_varied_sizes_and_every_boundary() {
    let sizes: Vec<_> = (0..137)
        .map(|i| {
            if i % 4 == 0 {
                13.25
            } else if i % 7 == 0 {
                71.5
            } else {
                32.0
            }
        })
        .collect();
    let mut axis = SparseAxis::new(sizes.len(), 32.0).unwrap();
    axis.replace(sizes.len(), 32.0, sizes.iter().copied().enumerate())
        .unwrap();
    let mut offset = 0.0;
    for (index, &height) in sizes.iter().enumerate() {
        assert_eq!(axis.offset_of(index), Some(offset));
        assert_eq!(axis.extent_at(index), Some(height));
        assert_eq!(axis.index_at(offset), Some(index));
        assert_eq!(axis.index_at(offset + height - 0.001), Some(index));
        offset += height;
    }
    assert_eq!(axis.offset_of(sizes.len()), Some(offset));
    assert_eq!(axis.index_at(offset), None);
    for start in (0..offset as usize).step_by(13) {
        let view = axis
            .range(
                start as f64,
                87.5,
                VirtualOverscan {
                    before: 3,
                    after: 5,
                },
                64,
            )
            .unwrap();
        let mut top = 0.0;
        let mut visible = Vec::new();
        for (index, &height) in sizes.iter().enumerate() {
            if top + height > view.scroll && top < view.scroll + 87.5 {
                visible.push(index)
            }
            top += height;
        }
        assert_eq!(view.visible.clone().collect::<Vec<_>>(), visible);
        assert!(
            view.materialized.start <= view.visible.start
                && view.materialized.end >= view.visible.end
        );
    }
}

#[test]
fn invalid_sparse_replacement_and_capacity_are_atomic() {
    let mut axis = SparseAxis::new(10, 32.0).unwrap();
    axis.set_extent(3, Some(64.0)).unwrap();
    let before = axis.clone();
    for entries in [
        vec![(3, 0.0)],
        vec![(3, f64::NAN)],
        vec![(10, 5.0)],
        vec![(2, 7.0), (2, 8.0)],
    ] {
        assert!(axis.replace(10, 32.0, entries).is_err());
        assert_eq!(axis, before);
    }
    assert!(axis.replace(MAX_VIRTUAL_ITEMS + 1, 32.0, []).is_err());
    assert_eq!(axis, before);
    assert!(axis.replace(MAX_VIRTUAL_ITEMS, 1_000_000.0, []).is_err());
    assert_eq!(axis, before);
    assert!(
        axis.replace(
            MAX_MEASURED_EXTENTS + 1,
            32.0,
            (0..=MAX_MEASURED_EXTENTS).map(|i| (i, 32.0))
        )
        .is_err()
    );
    assert_eq!(axis, before);
    assert!(axis.set_extent(0, Some(f64::INFINITY)).is_err());
    assert_eq!(axis, before);
}

#[test]
fn viewport_budget_trims_overscan_but_never_omits_visible_items() {
    let axis = SparseAxis::new(100, 1.0).unwrap();
    let range = axis
        .range(
            40.0,
            10.0,
            VirtualOverscan {
                before: 10,
                after: 10,
            },
            12,
        )
        .unwrap();
    assert_eq!(range.visible, 40..50);
    assert_eq!(range.materialized.len(), 12);
    assert!(range.materialized.contains(&40) && range.materialized.contains(&49));
    assert_eq!(
        axis.range(0.0, 20.0, VirtualOverscan::default(), 19),
        Err(VirtualError::Capacity)
    );
    let empty = axis
        .range(10.0, 0.0, VirtualOverscan::default(), 10)
        .unwrap();
    assert!(empty.visible.is_empty() && empty.materialized.is_empty());
    assert_eq!(
        axis.range(1e30, 10.0, VirtualOverscan::default(), 20)
            .unwrap()
            .visible,
        90..100
    );
    assert_eq!(
        axis.range(-10.0, 10.0, VirtualOverscan::default(), 20)
            .unwrap()
            .visible,
        0..10
    );
    assert!(
        axis.range(f64::NAN, 10.0, VirtualOverscan::default(), 20)
            .is_err()
    );
    assert!(
        axis.range(0.0, -1.0, VirtualOverscan::default(), 20)
            .is_err()
    );
}

#[test]
fn anchors_follow_keys_through_insert_remove_reorder_and_measurement_change() {
    let mut axis = SparseAxis::new(5, 32.0).unwrap();
    let before = ["a", "b", "c", "d", "e"];
    let anchor = StableScrollAnchor::capture(&axis, 71.5, |i| Some(before[i])).unwrap();
    assert_eq!(anchor.key, "c");
    assert_eq!(anchor.within_item, 7.5);
    let next = ["new", "e", "d", "a", "c", "b"];
    axis.replace(next.len(), 32.0, [(0, 88.0), (1, 16.0)])
        .unwrap();
    let restored = anchor.restore(&axis, |key| next.iter().position(|i| i == key));
    assert_eq!(restored, 175.5);
    assert_eq!(axis.index_at(restored), Some(4));
    let removed = ["a", "b", "d", "e"];
    axis.replace(removed.len(), 32.0, []).unwrap();
    assert_eq!(
        anchor.restore(&axis, |key| removed.iter().position(|i| i == key)),
        71.5
    );
    axis.replace(1, 2.0, []).unwrap();
    let restored = anchor.restore(&axis, |_| None);
    assert!(restored < 2.0);
    assert_eq!(axis.index_at(restored), Some(0));
    axis.replace(0, 32.0, []).unwrap();
    assert_eq!(anchor.restore(&axis, |_| None), 0.0);
}

#[test]
fn billion_item_geometry_preserves_small_extents_near_large_offsets() {
    let mut axis = SparseAxis::new(1_000_000_000, 1000.0).unwrap();
    axis.set_extent(999_999_998, Some(1.0)).unwrap();
    let top = axis.offset_of(999_999_998).unwrap();
    assert_eq!(axis.index_at(top), Some(999_999_998));
    assert_eq!(axis.index_at(top + 0.5), Some(999_999_998));
    assert_eq!(axis.index_at(top + 1.0), Some(999_999_999));
    assert_eq!(axis.override_count(), 1);
}

#[test]
fn recycling_keeps_same_key_identity_invalidates_reuse_and_bounds_allocations() {
    let mut slots = RecyclingSlots::new(8).unwrap();
    let first = slots.reconcile(&[1, 2, 3, 4]).unwrap();
    let old = first.active[0].lease;
    let retained = first.active[1].lease;
    let second = slots.reconcile(&[4, 2, 5]).unwrap();
    assert_eq!(second.active[1].lease, retained);
    assert_eq!(slots.resolve(old), None);
    assert_eq!(slots.resolve(retained), Some(&2));
    assert_eq!(second.assigned.len(), 1);
    assert_eq!(second.released.len(), 2);
    for start in 0..10_000 {
        let result = slots
            .reconcile(&(start..start + 8).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(result.active.len(), 8);
        assert!(slots.allocated_slots() <= 8);
        assert_eq!(slots.active_count(), 8);
        for item in result.active {
            assert_eq!(slots.resolve(item.lease), Some(&item.key));
        }
    }
    slots.reconcile(&[]).unwrap();
    assert_eq!(slots.active_count(), 0);
    assert!(slots.allocated_slots() <= 8);
}

#[test]
fn recycler_duplicate_capacity_and_generation_failure_preserve_assignments() {
    let mut slots = RecyclingSlots::new(2).unwrap();
    let first = slots.reconcile(&["a", "b"]).unwrap();
    for keys in [vec!["a", "a"], vec!["a", "b", "c"]] {
        assert!(slots.reconcile(&keys).is_err());
        for item in &first.active {
            assert_eq!(slots.resolve(item.lease), Some(&item.key));
        }
    }
    slots.sequence = u64::MAX;
    assert_eq!(
        slots.reconcile(&["c"]),
        Err(VirtualError::GenerationExhausted)
    );
    for item in &first.active {
        assert_eq!(slots.resolve(item.lease), Some(&item.key));
    }
    let same = slots.reconcile(&["b", "a"]).unwrap();
    assert_eq!(same.active[0].lease, first.active[1].lease);
    let other = RecyclingSlots::<&str>::new(2).unwrap();
    assert_eq!(other.resolve(first.active[0].lease), None);
}
