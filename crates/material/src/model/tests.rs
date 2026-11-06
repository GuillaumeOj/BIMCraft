use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use core_model::{AttributeValue, EntityId, Model};
use core_txn::{Document, ModelCommand, Origin};
use schema::Namespace;

use super::*;
use crate::{LawKind, MECHANICAL_SET, STEEL_SET, Source};

/// The core schema, loaded once for every test.
static CORE: LazyLock<Namespace> = LazyLock::new(|| {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/core");
    schema_gen::load_namespace(&dir).unwrap()
});

fn model() -> Model {
    Model::new(&CORE).unwrap()
}

fn s355() -> Material {
    let sourced =
        |value, clause| Sourced::new(value, Source::new("EN 1993-1-1:2005", clause).unwrap());
    let properties = BTreeMap::from([
        (Property::YieldStrength, sourced(355e6, "Table 3.1")),
        (Property::UltimateStrength, sourced(490e6, "Table 3.1")),
        (Property::YoungModulus, sourced(210e9, "3.2.6(1)")),
        (Property::PoissonRatio, sourced(0.3, "3.2.6(1)")),
        (
            Property::MassDensity,
            sourced(7850.0, "EN 1991-1-1, Table A.4"),
        ),
    ]);
    Material::new(
        "S355",
        Category::StructuralSteel,
        LawKind::SteelBilinear,
        Some("EN 1993-1-1:2005"),
        properties,
    )
    .unwrap()
}

const ID: EntityId = EntityId::new(7);

fn stored() -> Document<ModelCommand> {
    let mut document = Document::new(model());
    document.execute(commands(ID, &s355()), Origin::Ui).unwrap();
    document
}

fn set(document: &mut Document<ModelCommand>, command: ModelCommand) {
    document.execute(command, Origin::Ui).unwrap();
}

fn set_property(set: &str, property: &str, value: Option<AttributeValue>) -> ModelCommand {
    ModelCommand::SetProperty {
        id: ID,
        set: set.into(),
        property: property.into(),
        value,
    }
}

#[test]
fn a_material_round_trips_through_the_model() {
    let document = stored();
    assert_eq!(read(document.model(), ID), Ok(s355()));
    let modulus = document
        .model()
        .property(ID, MECHANICAL_SET, "YoungModulus");
    let source = Source::new("EN 1993-1-1:2005", "3.2.6(1)").unwrap();
    assert_eq!(
        modulus,
        Ok(Some(AttributeValue::sourced(
            AttributeValue::Real(210e9),
            source
        )))
    );
}

#[test]
fn storing_a_material_is_undoable() {
    let mut document = stored();
    document.undo().unwrap();
    assert_eq!(read(document.model(), ID), Err(ReadError::NotAMaterial));
    document.redo().unwrap();
    assert_eq!(read(document.model(), ID), Ok(s355()));
}

#[test]
fn every_property_maps_onto_the_core_schema() {
    let model = model();
    for property in Property::ALL {
        let set = model.property_set(property.property_set()).unwrap();
        let found = set.properties.iter().find(|p| p.name == property.name());
        assert!(found.is_some_and(|p| p.sourced), "{property:?}");
    }
}

#[test]
fn tampered_materials_are_rejected_with_the_reason() {
    let text = |value: &str| Some(AttributeValue::Text(value.into()));
    let attribute = |name: &str, value: &str| ModelCommand::Set {
        id: ID,
        attribute: name.into(),
        value: text(value),
    };
    let cases = [
        (
            attribute("category", "concrete"),
            ReadError::Invalid(MaterialError::LawNotForCategory {
                category: Category::Concrete,
                law: LawKind::SteelBilinear,
            }),
        ),
        (
            set_property(STEEL_SET, "YieldStrength", None),
            ReadError::Invalid(MaterialError::MissingProperty(Property::YieldStrength)),
        ),
    ];
    for (command, error) in cases {
        let mut document = stored();
        set(&mut document, command);
        assert_eq!(read(document.model(), ID), Err(error));
    }
}

#[test]
fn the_model_rejects_values_without_their_source() {
    let mut document = stored();
    let command = set_property(STEEL_SET, "YieldStrength", Some(AttributeValue::Real(1.0)));
    assert!(document.execute(command, Origin::Ui).is_err());
    assert_eq!(read(document.model(), ID), Ok(s355()));
}

#[test]
fn the_model_rejects_unknown_categories_and_laws() {
    for (attribute, value) in [("category", "timber"), ("behaviour_law", "plastic")] {
        let mut document = stored();
        let command = ModelCommand::Set {
            id: ID,
            attribute: attribute.into(),
            value: Some(AttributeValue::Text(value.into())),
        };
        assert!(
            document.execute(command, Origin::Ui).is_err(),
            "{attribute}"
        );
        assert_eq!(read(document.model(), ID), Ok(s355()));
    }
}

#[test]
fn categories_and_laws_match_the_core_enumerations() {
    let items = |name: &str| -> Vec<&str> {
        CORE.enumerations[name]
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect()
    };
    let categories: Vec<&str> = Category::ALL
        .iter()
        .map(|category| category.name())
        .collect();
    let laws: Vec<&str> = LawKind::ALL.iter().map(|law| law.name()).collect();
    assert_eq!(categories, items("MaterialCategory"));
    assert_eq!(laws, items("BehaviourLaw"));
}

#[test]
fn read_errors_explain_without_values() {
    assert_eq!(
        ReadError::NotAMaterial.to_string(),
        "the object is not a material"
    );
    assert_eq!(
        ReadError::UnknownLaw.to_string(),
        "the material's behaviour law is unknown"
    );
}
