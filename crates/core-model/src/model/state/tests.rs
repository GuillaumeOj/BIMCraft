use super::*;
use crate::Component;
use crate::model::tests::{
    CONTAINMENT, NO_VALUES, STOREY, WALLS, id, model, namespace, text, wall_common,
};
use crate::model::type_tests::{COMPONENT, WALL_TYPE, component};

/// The test model with a wall type, a typed wall carrying a property set, and a containment.
fn detailed() -> Model {
    let mut model = model();
    model.create_component(id(COMPONENT), component()).unwrap();
    model
        .create_type(id(WALL_TYPE), id(COMPONENT), NO_VALUES)
        .unwrap();
    model
        .assign_type(id(WALLS[0]), Some(id(WALL_TYPE)))
        .unwrap();
    model.set(id(WALLS[0]), "name", Some(text("W1"))).unwrap();
    model
        .attach_property_set(id(WALLS[0]), "Pset_WallCommon", [("Reference", text("R1"))])
        .unwrap();
    let roles = [
        ("structure", vec![id(STOREY)]),
        ("elements", vec![id(WALLS[0]), id(WALLS[1])]),
    ];
    model
        .relate(id(CONTAINMENT), "Containment", roles, NO_VALUES)
        .unwrap();
    model
}

#[test]
fn the_state_holds_everything_about_an_object() {
    let model = detailed();
    let wall = model.state(id(WALLS[0])).unwrap();
    assert_eq!(wall.entity, "Wall");
    assert_eq!(wall.values, [("name".to_owned(), text("W1"))]);
    assert_eq!(wall.roles, Vec::<Vec<EntityId>>::new());
    assert_eq!(wall.type_of, Some(id(WALL_TYPE)));
    assert_eq!(wall.component, None);
    let (set, values) = &wall.properties[0];
    assert_eq!(set, "Pset_WallCommon");
    assert!(values.contains(&("Reference".to_owned(), text("R1"))));

    let wall_type = model.state(id(WALL_TYPE)).unwrap();
    assert_eq!(wall_type.entity, "WallType");
    assert_eq!(wall_type.component, Some(id(COMPONENT)));

    let containment = model.state(id(CONTAINMENT)).unwrap();
    assert_eq!(
        containment.roles,
        [vec![id(STOREY)], vec![id(WALLS[0]), id(WALLS[1])]]
    );
    assert_eq!(model.state(id(999)), None);
    assert_eq!(model.ids().count(), model.len());
}

#[test]
fn models_with_the_same_content_are_equal_whatever_the_storage_order() {
    let model = detailed();
    let mut other = detailed();
    assert_eq!(other, model);
    // Removing and restoring moves rows, but not the content.
    let removal = other.remove(id(WALLS[0])).unwrap();
    assert_ne!(other, model);
    other.restore(removal).unwrap();
    assert_eq!(other, model);
}

#[test]
fn any_difference_makes_models_unequal() {
    let model = detailed();
    let edits: [fn(&mut Model); 6] = [
        |model| {
            model.set(id(WALLS[1]), "name", Some(text("W2"))).unwrap();
        },
        |model| {
            model.assign_type(id(WALLS[0]), None).unwrap();
        },
        |model| {
            model
                .detach_property_set(id(WALLS[0]), "Pset_WallCommon")
                .unwrap();
        },
        |model| {
            model
                .remove_participant(id(CONTAINMENT), "elements", id(WALLS[1]))
                .unwrap();
        },
        |model| {
            model.create_component(id(501), component()).unwrap();
        },
        |model| {
            let custom = schema::PropertySet {
                name: "Acme_Checks".into(),
                ..wall_common()
            };
            model.define_property_set(custom).unwrap();
        },
    ];
    for edit in edits {
        let mut changed = model.clone();
        edit(&mut changed);
        assert_ne!(changed, model);
    }
    assert_ne!(Model::new(&namespace()).unwrap(), model);
}

#[test]
fn removal_reach_lists_what_a_removal_may_change() {
    let model = detailed();
    assert_eq!(
        model.removal_reach(id(STOREY)),
        [id(STOREY), id(CONTAINMENT)]
    );
    assert_eq!(
        model.removal_reach(id(WALL_TYPE)),
        [id(WALL_TYPE), id(WALLS[0])]
    );
    assert_eq!(model.removal_reach(id(999)), []);
}

#[test]
fn components_serialise() {
    let json = serde_json::to_string(&component()).unwrap();
    assert_eq!(
        serde_json::from_str::<Component>(&json).unwrap(),
        component()
    );
}
