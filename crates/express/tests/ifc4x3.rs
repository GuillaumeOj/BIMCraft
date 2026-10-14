//! Parses and resolves the official IFC 4.3 schema vendored in `interop-ifc`, and checks known
//! declarations against the standard.

// Tests read the vendored schema, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::Path;

use express::model::{AttrType, Model, TypeKind};
use express::{ast, parse, resolve};

fn source() -> std::io::Result<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../interop-ifc/schema/IFC4X3_ADD2.exp");
    std::fs::read_to_string(path)
}

/// The resolved schema, or `None` when it cannot be read, parsed or resolved.
fn model() -> Option<Model> {
    let source = source().ok()?;
    resolve(&parse(&source).ok()?).ok()
}

/// Each attribute's name and whether it is derived, in STEP order.
fn attributes(model: &Model, entity: &str) -> Vec<(String, bool)> {
    model.entity(entity).map_or_else(Vec::new, |entity| {
        entity
            .attributes
            .iter()
            .map(|attribute| (attribute.name.clone(), attribute.derived))
            .collect()
    })
}

fn names(attributes: &[(String, bool)]) -> Vec<&str> {
    attributes.iter().map(|(name, _)| name.as_str()).collect()
}

#[test]
fn parses_every_declaration() {
    let schema = parse(&source().unwrap()).unwrap();
    assert_eq!(schema.name.name, "IFC4X3_ADD2");
    let count = |matches: fn(&ast::Declaration) -> bool| {
        schema
            .declarations
            .iter()
            .filter(|declaration| matches(declaration))
            .count()
    };
    assert_eq!(count(|d| matches!(d, ast::Declaration::Type(_))), 436);
    assert_eq!(count(|d| matches!(d, ast::Declaration::Entity(_))), 876);
    assert_eq!(count(|d| matches!(d, ast::Declaration::Function(_))), 48);
    assert_eq!(count(|d| matches!(d, ast::Declaration::Rule(_))), 2);
}

#[test]
fn resolves_the_schema() {
    let model = model().unwrap();
    assert_eq!(model.name, "IFC4X3_ADD2");
    assert_eq!(model.types.len(), 436);
    assert_eq!(model.entities.len(), 876);
    assert_eq!(model.algorithms.len(), 50);
}

#[test]
fn flattens_a_wall() {
    let model = model().unwrap();
    let wall = attributes(&model, "IfcWall");
    assert_eq!(
        names(&wall),
        [
            "GlobalId",
            "OwnerHistory",
            "Name",
            "Description",
            "ObjectType",
            "ObjectPlacement",
            "Representation",
            "Tag",
            "PredefinedType",
        ]
    );
    assert!(wall.iter().all(|(_, derived)| !derived));
    let entity = model.entity("IFCWALL").unwrap();
    assert!(!entity.is_abstract);
    assert_eq!(entity.supertypes, ["IfcBuiltElement"]);
    assert!(model.entity("IfcRoot").unwrap().is_abstract);
}

#[test]
fn marks_attributes_that_subtypes_derive() {
    let model = model().unwrap();
    let unit = attributes(&model, "IfcSIUnit");
    assert_eq!(names(&unit), ["Dimensions", "UnitType", "Prefix", "Name"]);
    assert!(
        unit[0].1,
        "IfcSIUnit derives the dimensions of IfcNamedUnit"
    );
    let context = attributes(&model, "IfcGeometricRepresentationSubContext");
    let derived: Vec<&str> = context
        .iter()
        .filter(|(_, derived)| *derived)
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        derived,
        [
            "CoordinateSpaceDimension",
            "Precision",
            "WorldCoordinateSystem",
            "TrueNorth"
        ]
    );
}

#[test]
fn resolves_types() {
    let model = model().unwrap();
    assert_eq!(
        model.type_def("IfcLabel").unwrap().kind,
        TypeKind::Defined(AttrType::String)
    );
    let TypeKind::Enumeration(items) = &model.type_def("IfcWallTypeEnum").unwrap().kind else {
        panic!("an enumeration");
    };
    assert!(items.iter().any(|item| item == "SOLIDWALL"));
    assert_eq!(items.last().map(String::as_str), Some("NOTDEFINED"));
    let members = model.select_members("IfcValue");
    assert!(members.iter().any(|member| member == "IfcLengthMeasure"));
    assert!(members.iter().any(|member| member == "IfcLabel"));
}
