//! Every IFC mapping of the schema names real IFC 4.3 classes (CORE-10): the entity and type
//! entity exist and can be instantiated, and the predefined type is an item of the entity's
//! predefined-type enumeration. The schema crates only check that the names are well formed.

// Tests read the schema files, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::Path;
use std::sync::LazyLock;

use express::Model;
use express::model::{AttrType, TypeKind};
use schema::Namespace;
use schema_gen::{Source, load_dir, parse_sources};

/// The IFC attribute that holds an entity's predefined type.
const PREDEFINED_TYPE: &str = "PredefinedType";

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The IFC 4.3 schema, parsed once for every test.
static IFC: LazyLock<Result<Model, String>> = LazyLock::new(ifc_schema);

fn ifc_schema() -> Result<Model, String> {
    let path = crate_dir().join("schema/IFC4X3_ADD2.exp");
    let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let schema = express::parse(&source).map_err(|error| error.to_string())?;
    express::resolve(&schema).map_err(|errors| format!("{errors:?}"))
}

fn namespace(dir: &Path) -> Result<Namespace, String> {
    let sources = load_dir(dir).map_err(|error| error.to_string())?;
    parse_sources(&sources).map_err(|errors| format!("{errors:?}"))
}

/// The items of the enumeration an entity's `PredefinedType` attribute takes, if it has one.
fn predefined_types<'a>(ifc: &'a Model, entity: &str) -> Option<&'a [String]> {
    let attribute = ifc
        .entity(entity)?
        .attributes
        .iter()
        .find(|attribute| attribute.name == PREDEFINED_TYPE)?;
    let AttrType::Named(name) = &attribute.ty else {
        return None;
    };
    match &ifc.type_def(name)?.kind {
        TypeKind::Enumeration(items) => Some(items),
        _ => None,
    }
}

/// Checks that `entity` names an IFC entity that can be instantiated and, when it has a
/// predefined type, that the mapping gives one of its values.
fn check_class(
    ifc: &Model,
    owner: &str,
    entity: &str,
    predefined_type: Option<&str>,
    problems: &mut Vec<String>,
) {
    match ifc.entity(entity) {
        None => problems.push(format!("{owner}: {entity} is not an IFC 4.3 entity")),
        Some(found) if found.name != entity => {
            problems.push(format!(
                "{owner}: {entity} is spelled {} in IFC 4.3",
                found.name
            ));
        }
        Some(found) if found.is_abstract => {
            problems.push(format!("{owner}: {entity} is abstract"));
        }
        Some(_) => match (predefined_types(ifc, entity), predefined_type) {
            (Some(items), Some(value)) if !items.iter().any(|item| item == value) => {
                problems.push(format!(
                    "{owner}: {value} is not a predefined type of {entity}"
                ));
            }
            (Some(_), None) => {
                problems.push(format!("{owner}: {entity} needs a predefined type"));
            }
            (None, Some(_)) => {
                problems.push(format!("{owner}: {entity} has no predefined type"));
            }
            _ => {}
        },
    }
}

/// Every problem with the IFC mappings of `namespace`, one line each.
fn problems(ifc: &Model, namespace: &Namespace) -> Vec<String> {
    let mut problems = Vec::new();
    for entity in namespace.entities.values() {
        // Validation already rejects an entity with no mapping.
        let Some(mapping) = &entity.ifc else {
            continue;
        };
        let predefined_type = mapping.predefined_type.as_deref();
        check_class(
            ifc,
            &entity.name,
            &mapping.entity,
            predefined_type,
            &mut problems,
        );
        if let Some(type_entity) = &mapping.type_entity {
            check_class(
                ifc,
                &entity.name,
                type_entity,
                predefined_type,
                &mut problems,
            );
        }
    }
    problems
}

/// The definition file of an element entity named `name`, with `ifc` as its `[ifc]` section and
/// `extra` after it.
fn entity(name: &str, ifc: &str, extra: &str) -> Source {
    Source {
        path: format!("entities/{name}.toml"),
        contents: format!(
            "format = 1\n[entity]\nname = \"{name}\"\nkind = \"element\"\n[ifc]\n{ifc}\n{extra}"
        ),
    }
}

#[test]
fn the_core_schema_maps_onto_ifc() {
    let core = namespace(&crate_dir().join("../../schema/core")).unwrap();
    assert_eq!(problems(IFC.as_ref().unwrap(), &core), Vec::<String>::new());
}

/// The schema-gen sample covers predefined types, `USERDEFINED`, derived and explicit type
/// entities, a spatial element and relationships.
#[test]
fn the_sample_schema_maps_onto_ifc() {
    let sample = crate_dir().join("../schema-gen/tests/fixtures/sample");
    assert_eq!(
        problems(IFC.as_ref().unwrap(), &namespace(&sample).unwrap()),
        Vec::<String>::new()
    );
}

#[test]
fn reports_mappings_ifc_does_not_have() {
    let sources = [
        Source {
            path: "namespace.toml".into(),
            contents: "format = 1\n[namespace]\nname = \"core\"\nversion = 1\n".into(),
        },
        entity(
            "A",
            "entity = \"IfcWal\"\npredefined_type = \"STANDARD\"",
            "",
        ),
        entity(
            "B",
            "entity = \"IfcWall\"\npredefined_type = \"CURVED\"",
            "",
        ),
        entity(
            "C",
            "entity = \"IfcBuiltElement\"\npredefined_type = \"X\"",
            "",
        ),
        entity("D", "entity = \"IfcWall\"", ""),
        entity("E", "entity = \"IfcElement\"", ""),
        entity("F", "entity = \"IfcSite\"\npredefined_type = \"SOLID\"", ""),
        entity(
            "G",
            "entity = \"IfcSlab\"\npredefined_type = \"FLOOR\"\ntype_entity = \"IfcSlabTyp\"",
            "[type]\n",
        ),
        entity("H", "entity = \"IfcCartesianpoint\"", ""),
    ];
    let namespace = parse_sources(&sources).unwrap();
    let mut ifc = IFC.as_ref().unwrap().clone();
    // A schema entity whose `PredefinedType` is not an enumeration is treated as having none.
    ifc.entities
        .get_mut("IFCSITE")
        .unwrap()
        .attributes
        .push(express::model::Attribute {
            name: PREDEFINED_TYPE.to_owned(),
            declared_by: "IfcSite".to_owned(),
            declared_name: PREDEFINED_TYPE.to_owned(),
            ty: AttrType::String,
            optional: true,
            derived: false,
        });
    assert_eq!(
        problems(&ifc, &namespace),
        [
            "A: IfcWal is not an IFC 4.3 entity",
            "B: CURVED is not a predefined type of IfcWall",
            "C: IfcBuiltElement has no predefined type",
            "D: IfcWall needs a predefined type",
            "E: IfcElement is abstract",
            "F: IfcSite has no predefined type",
            "G: IfcSlabTyp is not an IFC 4.3 entity",
            "H: IfcCartesianpoint is spelled IfcCartesianPoint in IFC 4.3",
        ]
    );
}
