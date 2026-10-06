use super::StdPackage;

pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.collections",
    deps: &["system.core", "system.primitives"],
    generators: &[],
    files: &[
        (
            "<std>/system/collections/list.dream",
            include_str!("../system/collections/list.dream"),
        ),
        (
            "<std>/system/collections/list_iterator.dream",
            include_str!("../system/collections/list_iterator.dream"),
        ),
        (
            "<std>/system/collections/map_slot.dream",
            include_str!("../system/collections/map_slot.dream"),
        ),
        (
            "<std>/system/collections/map.dream",
            include_str!("../system/collections/map.dream"),
        ),
        (
            "<std>/system/collections/key_value_pair.dream",
            include_str!("../system/collections/key_value_pair.dream"),
        ),
        (
            "<std>/system/collections/map_iterator.dream",
            include_str!("../system/collections/map_iterator.dream"),
        ),
        (
            "<std>/system/collections/sorted_map.dream",
            include_str!("../system/collections/sorted_map.dream"),
        ),
        (
            "<std>/system/collections/sorted_map_iterator.dream",
            include_str!("../system/collections/sorted_map_iterator.dream"),
        ),
        (
            "<std>/system/collections/set.dream",
            include_str!("../system/collections/set.dream"),
        ),
        (
            "<std>/system/collections/set_iterator.dream",
            include_str!("../system/collections/set_iterator.dream"),
        ),
        (
            "<std>/system/collections/queue.dream",
            include_str!("../system/collections/queue.dream"),
        ),
        (
            "<std>/system/collections/queue_iterator.dream",
            include_str!("../system/collections/queue_iterator.dream"),
        ),
        (
            "<std>/system/collections/priority_queue.dream",
            include_str!("../system/collections/priority_queue.dream"),
        ),
        (
            "<std>/system/collections/priority_queue_iterator.dream",
            include_str!("../system/collections/priority_queue_iterator.dream"),
        ),
        (
            "<std>/system/collections/stack.dream",
            include_str!("../system/collections/stack.dream"),
        ),
        (
            "<std>/system/collections/collection_query.dream",
            include_str!("../system/collections/collection_query.dream"),
        ),
        (
            "<std>/system/collections/seq.dream",
            include_str!("../system/collections/seq.dream"),
        ),
    ],
};
