//! What an element's mesh always depends on, read from attribute values and registries.

use core_model::{AttributeValue, Source};

use super::fixture::{Calls, generators, id};
use crate::scene::references;
use crate::{Scene, View};

#[test]
fn references_are_found_in_lists_and_sourced_values_however_deep() {
    let source = Source::new("EN 1992-1-1", "3.1.2").unwrap();
    let values = [
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
    let mut found: Vec<_> = references(values.iter()).collect();
    found.sort();
    assert_eq!(found, vec![id(1), id(2), id(3), id(4)]);
}

#[test]
fn a_scene_describes_itself_by_generator_paths_not_by_generators() {
    let scene = Scene::new(generators(&Calls::default()), View::new("body", 100));
    let description = format!("{scene:?}");
    assert!(description.starts_with("Scene {"));
    assert!(
        description.contains(r#"{"column::axis", "column::coarse", "column::fine", "wall::body"}"#)
    );
    assert!(description.contains(r#"View { context: "body", lod: 100 }"#));
}
