//! What an element's mesh always depends on, read from attribute values and registries.

use core_model::{AttributeValue, Source};

use super::fixture::{calls_debug, id};
use crate::scene::references;

#[test]
fn references_are_found_in_lists_and_sourced_values_however_deep() {
    let source = Source::new("EN 1992-1-1", "3.1.2").unwrap();
    let values = vec![
        ("a".to_owned(), AttributeValue::Ref(id(1))),
        (
            "b".to_owned(),
            AttributeValue::List(vec![
                AttributeValue::Real(1.0),
                AttributeValue::Ref(id(2)),
                AttributeValue::List(vec![AttributeValue::Ref(id(3))]),
            ]),
        ),
        (
            "c".to_owned(),
            AttributeValue::sourced(AttributeValue::Ref(id(4)), source),
        ),
        (
            "d".to_owned(),
            AttributeValue::Text("not a reference".into()),
        ),
    ];
    let mut found: Vec<_> = references(&values).collect();
    found.sort();
    assert_eq!(found, vec![id(1), id(2), id(3), id(4)]);
}

#[test]
fn generators_and_scenes_describe_themselves_without_content() {
    assert_eq!(
        calls_debug(),
        "Scene { generators: {\"column::axis\", \"column::coarse\", \"column::fine\", \"wall::body\"}, \
         view: View { context: \"body\", lod: 100 }, graph: DependencyGraph { nodes: {} }, \
         meshes: {}, failures: {}, stale: {}, dirty: {}, reported: {} }"
    );
}
