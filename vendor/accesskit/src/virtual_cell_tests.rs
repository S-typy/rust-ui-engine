// Copyright 2026 rust-ui-engine contributors. Apache-2.0.
use super::*;

#[test]
fn virtual_grid_metadata_survives_clone_and_clone_from() {
    let mut node = Node::new(Role::Grid);
    node.add_action(Action::RealizeCell);
    node.set_virtualization_generation(42);
    node.set_virtual_cell_spans(vec![VirtualCellSpan {
        row: 900_000,
        column: 4,
        rows: 3,
        columns: 2,
    }]);
    let mut copy = Node::new(Role::Unknown);
    copy.clone_from(&node);
    assert_eq!(copy, node);
    assert_eq!(copy.clone(), node);
    assert_eq!(copy.virtualization_generation(), Some(42));
    assert_eq!(copy.virtual_cell_spans()[0].row, 900_000);
    assert_eq!(
        action_mask_to_action_vec(copy.actions),
        vec![Action::RealizeCell]
    );
}

#[cfg(feature = "schemars")]
#[test]
fn virtual_grid_metadata_and_action_have_lossless_serialized_contract() {
    let mut node = Node::new(Role::Grid);
    node.set_virtualization_generation(42);
    node.set_virtual_cell_spans(vec![VirtualCellSpan {
        row: 999_999,
        column: 16_383,
        rows: 1,
        columns: 1,
    }]);
    let encoded = serde_json::to_string(&node).unwrap();
    assert_eq!(serde_json::from_str::<Node>(&encoded).unwrap(), node);
    let request = ActionRequest {
        action: Action::RealizeCell,
        target_tree: TreeId::ROOT,
        target_node: NodeId(3),
        data: Some(ActionData::RealizeCell {
            row: 999_999,
            column: 16_383,
            generation: 42,
        }),
    };
    let encoded = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<ActionRequest>(&encoded).unwrap(),
        request
    );
    let schema = schemars::schema_for!(Node);
    assert!(
        serde_json::to_string(&schema)
            .unwrap()
            .contains("virtualCellSpans")
    );
}
