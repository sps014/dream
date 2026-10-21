use super::*;
use proptest::prelude::*;

#[test]
fn geometry_decodes_triangles_indexed_triangles_and_boxes() {
    assert_eq!(
        decode_geometry(&[0, 7, 3, 0, 0, 1]).unwrap(),
        Geometry {
            kind: GeometryKind::Triangles,
            vertices: 7,
            vertex_count: 3,
            indices: None,
            opaque: true,
        }
    );
    let indexed = decode_geometry(&[0, 7, 4, 9, 6, 0]).unwrap();
    assert_eq!(indexed.indices, Some((9, 6)));
    assert!(!indexed.opaque);
    let boxes = decode_geometry(&[1, 7, 4, 0, 0, 1]).unwrap();
    assert_eq!(boxes.kind, GeometryKind::Boxes);
}

#[test]
fn geometry_rejects_malformed_descriptions() {
    for words in [
        &[0, 7, 3, 0, 0][..],
        &[2, 7, 3, 0, 0, 1],
        &[0, 7, 0, 0, 0, 1],
        &[0, 7, -3, 0, 0, 1],
        &[0, 7, 4, 0, 0, 1],
        &[0, 7, 4, 9, 5, 1],
        &[0, 7, 4, 9, 0, 1],
        &[1, 7, 3, 0, 0, 1],
        &[1, 7, 4, 9, 6, 1],
        &[0, 7, 3, 0, 0, 2],
    ] {
        assert!(decode_geometry(words).is_err(), "{words:?}");
    }
}

fn identity() -> [f32; 12] {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]
}

#[test]
fn instances_pair_records_with_row_major_transforms() {
    let mut transforms = identity().to_vec();
    let mut moved = identity();
    moved[3] = 5.0;
    transforms.extend(moved);
    let decoded = decode_instances(&[11, 1, 255, 0, 0, 12, 2, 1, 3, 4], &transforms).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].bottom_level, 11);
    assert_eq!(decoded[0].mask, 255);
    assert_eq!(decoded[1].hit_group_offset, 3);
    assert_eq!(decoded[1].flags, 4);
    assert_eq!(decoded[1].transform[3], 5.0);
}

#[test]
fn instances_reject_mismatched_or_out_of_range_fields() {
    let t = identity();
    assert!(decode_instances(&[], &[]).is_err());
    assert!(decode_instances(&[1, 0, 255, 0], &t).is_err());
    assert!(decode_instances(&[1, 0, 255, 0, 0], &t[..11]).is_err());
    assert!(decode_instances(&[1, 1 << 24, 255, 0, 0], &t).is_err());
    assert!(decode_instances(&[1, 0, 256, 0, 0], &t).is_err());
    assert!(decode_instances(&[1, 0, 255, -1, 0], &t).is_err());
    assert!(decode_instances(&[1, 0, 255, 0, 256], &t).is_err());
    let mut bad = t;
    bad[5] = f32::NAN;
    assert!(decode_instances(&[1, 0, 255, 0, 0], &bad).is_err());
}

#[test]
fn layout_builds_the_binding_table_in_program_order() {
    // Entries: 0 camera (raygen), 1 sky (miss), 2 shade (closest hit), 3 cutout (any hit),
    // 4 shadow (miss), 5 sphere (intersection), 6 tint (callable).
    let layout = Layout::decode(&[0, 2, 1, 4, 2, 2, 3, -1, 2, -1, 5, 1, 6], 7).unwrap();
    assert_eq!(layout.ray_generation, 0);
    assert_eq!(layout.misses, vec![1, 4]);
    assert_eq!(
        layout.hit_groups,
        vec![
            HitGroup {
                closest_hit: 2,
                any_hit: Some(3),
                intersection: None,
            },
            HitGroup {
                closest_hit: 2,
                any_hit: None,
                intersection: Some(5),
            },
        ]
    );
    assert_eq!(layout.callables, vec![6]);
    // The shared closest hit appears once; ray generation comes first.
    assert_eq!(
        layout.program_entries().unwrap(),
        vec![
            (0, Stage::RayGeneration),
            (1, Stage::Miss),
            (4, Stage::Miss),
            (2, Stage::ClosestHit),
            (3, Stage::AnyHit),
            (5, Stage::Intersection),
            (6, Stage::Callable),
        ]
    );
}

#[test]
fn layout_rejects_truncated_out_of_range_and_conflicting_tables() {
    assert!(
        Layout::decode(&[0, 0, 0], 1).is_err(),
        "missing callable count"
    );
    assert!(
        Layout::decode(&[0, 0, 0, 0, 9], 1).is_err(),
        "trailing words"
    );
    assert!(
        Layout::decode(&[1, 0, 0, 0], 1).is_err(),
        "ray generation out of range"
    );
    assert!(
        Layout::decode(&[0, 1, 5, 0, 0], 2).is_err(),
        "miss out of range"
    );
    assert!(
        Layout::decode(&[0, 0, 1, -1, -1, -1, 0], 2).is_err(),
        "hit group without closest hit"
    );
    assert!(Layout::decode(&[0, -1, 0, 0], 2).is_err(), "negative count");
    let conflicting = Layout::decode(&[0, 1, 1, 1, 1, -1, -1, 0], 2).unwrap();
    assert!(
        conflicting.program_entries().is_err(),
        "one entry as miss and closest hit"
    );
    assert!(
        Layout::decode(&[0, 0, 0, 0], 1)
            .unwrap()
            .program_entries()
            .is_ok()
    );
}

proptest! {
    #[test]
    fn decoders_never_panic(words in proptest::collection::vec(any::<i32>(), 0..24),
                            floats in proptest::collection::vec(any::<f32>(), 0..30),
                            entries in 0usize..8) {
        let _ = decode_geometry(&words);
        let _ = decode_instances(&words, &floats);
        if let Ok(layout) = Layout::decode(&words, entries) {
            let _ = layout.program_entries();
        }
    }
}
