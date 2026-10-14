//! Every way definitions can be wrong is reported, with the file and, when known, the line.

// Tests build throwaway namespace directories, which the ARCH-02 lints of clippy.toml flag.
#![allow(clippy::disallowed_methods)]

use std::path::PathBuf;

use schema::{EntityKind, Namespace, ValidationErrorKind};
use schema_gen::{
    Error, ErrorKind, JsonSchemaOptions, MAX_FILE_SIZE, RustOptions, Source, emit_json_schema,
    emit_rust, generate_rust, load_dir, parse_sources,
};

const NAMESPACE: &str = "format = 1\n[namespace]\nname = \"core\"\nversion = 1\n";

const STOREY: &str = r#"format = 1

[entity]
name = "Storey"
kind = "spatial"

[ifc]
entity = "IfcBuildingStorey"
"#;

fn source(path: &str, contents: &str) -> Source {
    Source {
        path: path.into(),
        contents: contents.into(),
    }
}

/// Parses a namespace made of `namespace.toml`, a storey, and `extra`.
fn parse_with(extra: &[(&str, &str)]) -> Result<Namespace, Vec<Error>> {
    let mut sources = vec![
        source("namespace.toml", NAMESPACE),
        source("entities/storey.toml", STOREY),
    ];
    sources.extend(extra.iter().map(|(path, contents)| source(path, contents)));
    parse_sources(&sources)
}

fn single_error(extra: &[(&str, &str)]) -> Error {
    let mut errors = parse_with(extra).err().unwrap_or_default();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    errors.remove(0)
}

/// Asserts the file, line and message of an error.
fn assert_error(error: &Error, file: &str, line: Option<usize>, message: &str) {
    assert_eq!(error.file.as_deref(), Some(file), "{error}");
    assert_eq!(error.line, line, "{error}");
    assert!(
        error.kind.to_string().contains(message),
        "\"{}\" should contain \"{message}\"",
        error.kind
    );
}

fn wall(attributes: &str) -> String {
    format!(
        "format = 1\n\n[entity]\nname = \"Wall\"\nkind = \"element\"\n\n[ifc]\nentity = \
         \"IfcWall\"\n{attributes}"
    )
}

#[test]
fn accepts_a_minimal_namespace() {
    let namespace = parse_with(&[]).unwrap();
    assert_eq!(namespace.name, "core");
    assert_eq!(namespace.entities["Storey"].kind, EntityKind::Spatial);
}

#[test]
fn requires_the_namespace_file() {
    let errors = parse_sources(&[source("entities/storey.toml", STOREY)]).unwrap_err();
    assert_eq!(
        errors,
        vec![Error {
            file: None,
            line: None,
            column: None,
            kind: ErrorKind::Definition("namespace.toml is missing".into()),
        }]
    );
}

#[test]
fn reports_a_broken_namespace_file_once() {
    let errors =
        parse_sources(&[source("namespace.toml", "format = 1\n[namespace\n")]).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(matches!(errors[0].kind, ErrorKind::Syntax(_)));
}

#[test]
fn reports_toml_syntax_errors_with_their_line() {
    let error = single_error(&[("entities/wall.toml", "format = 1\n\n[entity\n")]);
    assert_error(&error, "entities/wall.toml", Some(3), "");
    assert!(matches!(error.kind, ErrorKind::Syntax(_)));
    assert!(error.column.is_some());
}

#[test]
fn rejects_unknown_keys() {
    let error = single_error(&[(
        "entities/wall.toml",
        &wall("\n[[attribute]]\nname = \"height\"\nvalue = \"real\"\nunit = \"m\"\n"),
    )]);
    assert!(matches!(error.kind, ErrorKind::Syntax(_)));
    assert_error(
        &error,
        "entities/wall.toml",
        Some(13),
        "unknown field `unit`",
    );
}

#[test]
fn rejects_other_format_versions() {
    let error = single_error(&[(
        "entities/wall.toml",
        &wall("").replace("format = 1", "format = 2"),
    )]);
    assert_error(
        &error,
        "entities/wall.toml",
        Some(1),
        "format 2 is not supported; this version reads format 1",
    );
}

#[test]
fn rejects_files_outside_the_definition_directories() {
    let error = single_error(&[("notes/wall.toml", &wall(""))]);
    assert_error(&error, "notes/wall.toml", None, "unexpected file");
}

#[test]
fn rejects_oversized_files() {
    let huge = format!("{}\n# {}\n", wall(""), "x".repeat(MAX_FILE_SIZE));
    let error = single_error(&[("entities/wall.toml", &huge)]);
    assert_error(&error, "entities/wall.toml", None, "larger than");
}

#[test]
fn reports_bad_value_types_at_their_line() {
    let error = single_error(&[(
        "entities/wall.toml",
        &wall("\n[[attribute]]\nname = \"height\"\nvalue = \"list:float\"\n"),
    )]);
    assert_error(
        &error,
        "entities/wall.toml",
        Some(12),
        "unknown value type \"float\"",
    );
}

#[test]
fn rejects_required_attributes_with_defaults() {
    let error = single_error(&[(
        "entities/wall.toml",
        &wall(
            "\n[[attribute]]\nname = \"height\"\nvalue = \"real\"\nrequired = true\ndefault = 2.5\n",
        ),
    )]);
    assert_error(
        &error,
        "entities/wall.toml",
        Some(14),
        "both required and have a default",
    );
}

#[test]
fn rejects_defaults_that_are_not_plain_values() {
    for default in ["1979-05-27", "{ a = 1 }", "[[[[[1]]]]]"] {
        let attributes = format!(
            "\n[[attribute]]\nname = \"height\"\nvalue = \"list:list:list:list:real\"\ndefault = \
             {default}\n"
        );
        let error = single_error(&[("entities/wall.toml", &wall(&attributes))]);
        assert_eq!(error.line, Some(13), "{error}");
        assert!(matches!(error.kind, ErrorKind::Definition(_)), "{error}");
    }
}

#[test]
fn accepts_nested_list_defaults() {
    let attributes = "\n[[attribute]]\nname = \"grid\"\nvalue = \"list:list:integer\"\ndefault = \
                      [[1, 2], [3]]\n";
    assert!(parse_with(&[("entities/wall.toml", &wall(attributes))]).is_ok());
}

#[test]
fn rejects_unknown_role_kinds() {
    let relationship = r#"format = 1

[entity]
name = "Zoning"
kind = "relationship"

[ifc]
entity = "IfcRelAssignsToGroup"

[[role]]
name = "members"
targets = ["kind:zone"]
cardinality = "many"
"#;
    let error = single_error(&[("relationships/zoning.toml", relationship)]);
    assert_error(
        &error,
        "relationships/zoning.toml",
        Some(12),
        "unknown entity kind \"zone\"",
    );
}

#[test]
fn rejects_definitions_repeated_across_files() {
    let error = single_error(&[("relationships/storey.toml", STOREY)]);
    assert_error(
        &error,
        "relationships/storey.toml",
        Some(4),
        "entity \"Storey\" is already defined in entities/storey.toml",
    );

    let property_set = "format = 1\n[property_set]\nname = \"Pset_A\"\napplies_to = [\"Storey\"]\n";
    let error = single_error(&[
        ("property-sets/a.toml", property_set),
        ("property-sets/b.toml", property_set),
    ]);
    assert_error(
        &error,
        "property-sets/b.toml",
        Some(3),
        "property set \"Pset_A\" is already defined in property-sets/a.toml",
    );
}

#[test]
fn places_validation_errors_at_the_item() {
    let error = single_error(&[(
        "entities/wall.toml",
        &wall("\n[[attribute]]\nname = \"host\"\nvalue = \"ref:Slab\"\n"),
    )]);
    assert_error(
        &error,
        "entities/wall.toml",
        Some(11),
        "unknown entity \"Slab\"",
    );
    assert_eq!(
        error.to_string(),
        "entities/wall.toml:11:8: entity Wall, attribute host: unknown entity \"Slab\""
    );
}

#[test]
fn places_validation_errors_on_types_ifc_and_representations() {
    let typed = "format = 1\n\n[entity]\nname = \"Wall\"\nkind = \"element\"\n\n[type]\nname = \
                 \"Storey\"\n\n[ifc]\nentity = \"Wall\"\n\n[[representation]]\nlod = 100\ncontext \
                 = \"body\"\ngenerator = \"wall::body\"\ndepends_on = [\"x\"]\n";
    let errors = parse_with(&[("entities/wall.toml", typed)]).unwrap_err();
    let lines: Vec<_> = errors.iter().map(|error| error.line).collect();
    // [type] name clash, then the IFC name and the derived IFC type name, then depends_on.
    assert_eq!(
        lines,
        [Some(7), Some(10), Some(10), Some(14)],
        "{errors:#?}"
    );
}

#[test]
fn places_definition_level_errors_at_the_name() {
    let error = single_error(&[(
        "entities/wall.toml",
        "format = 1\n\n[entity]\nname = \"Wall\"\nkind = \"element\"\n",
    )]);
    assert_error(
        &error,
        "entities/wall.toml",
        Some(4),
        "needs an [ifc] mapping",
    );

    let property_set = "format = 1\n[property_set]\nname = \"Pset_A\"\napplies_to = [\"Wall\"]\n\n[[property]]\nname \
         = \"Mark\"\nvalue = \"ref:Wall\"\n";
    let errors = parse_with(&[("property-sets/a.toml", property_set)]).unwrap_err();
    let lines: Vec<_> = errors.iter().map(|error| error.line).collect();
    assert_eq!(lines, [Some(3), Some(7)], "{errors:#?}");
}

#[test]
fn places_namespace_errors_in_the_namespace_file() {
    let sources = [source(
        "namespace.toml",
        "format = 1\n[namespace]\nname = \"Core\"\nversion = 0\n",
    )];
    let errors = parse_sources(&sources).unwrap_err();
    assert_eq!(errors.len(), 2, "{errors:#?}");
    for error in &errors {
        assert_eq!(error.file.as_deref(), Some("namespace.toml"));
        assert_eq!(error.line, None);
    }
}

#[test]
fn reports_every_error_at_once() {
    let errors = parse_with(&[
        ("entities/a.toml", "format = 1\n[entity\n"),
        ("entities/b.toml", "format = 3\n"),
        ("notes.toml", ""),
    ])
    .unwrap_err();
    let files: Vec<_> = errors.iter().map(|error| error.file.as_deref()).collect();
    assert_eq!(
        files,
        [
            Some("entities/a.toml"),
            Some("entities/b.toml"),
            Some("notes.toml")
        ]
    );
}

fn options() -> RustOptions {
    RustOptions {
        id_type: "Id".into(),
    }
}

#[test]
fn emitting_validates_first() {
    let mut namespace = parse_with(&[]).unwrap();
    namespace.version = 0;
    let errors = emit_rust(&namespace, &options()).unwrap_err();
    assert!(matches!(
        &errors[0].kind,
        ErrorKind::Validation(error) if error.kind == ValidationErrorKind::ZeroVersion
    ));
}

fn json_options() -> JsonSchemaOptions {
    JsonSchemaOptions {
        id: "urn:test".into(),
        object_id: serde_json::json!({ "type": "integer" }),
    }
}

#[test]
fn json_schema_emitting_validates_first() {
    let mut namespace = parse_with(&[]).unwrap();
    namespace.version = 0;
    let errors = emit_json_schema(&namespace, &json_options()).unwrap_err();
    assert!(matches!(
        &errors[0].kind,
        ErrorKind::Validation(error) if error.kind == ValidationErrorKind::ZeroVersion
    ));
}

#[test]
fn json_schema_keeps_the_object_id_definition_free() {
    let entity = STOREY
        .replace("\"Storey\"", "\"ObjectId\"")
        .replace("IfcBuildingStorey", "IfcSpace");
    let namespace = parse_with(&[("entities/object_id.toml", &entity)]).unwrap();
    let errors = emit_json_schema(&namespace, &json_options()).unwrap_err();
    assert_eq!(
        errors[0].to_string(),
        "two JSON Schema definitions are named `ObjectId`"
    );
}

#[test]
fn escapes_keywords_and_rejects_unusable_names() {
    let namespace = parse_with(&[(
        "entities/wall.toml",
        &wall("\n[[attribute]]\nname = \"type\"\nvalue = \"text\"\n"),
    )])
    .unwrap();
    let code = emit_rust(&namespace, &options()).unwrap();
    assert!(code.contains("    pub r#type: Option<String>,\n"), "{code}");

    let namespace = parse_with(&[(
        "entities/wall.toml",
        &wall("\n[[attribute]]\nname = \"self\"\nvalue = \"text\"\n"),
    )])
    .unwrap();
    let errors = emit_rust(&namespace, &options()).unwrap_err();
    assert_eq!(
        errors[0].to_string(),
        "`self` in `Wall` cannot be a Rust field name"
    );
}

#[test]
fn rejects_struct_names_the_generated_code_relies_on() {
    for name in ["Option", "String", "Vec", "Self"] {
        let entity = STOREY
            .replace("\"Storey\"", &format!("{name:?}"))
            .replace("IfcBuildingStorey", "IfcSpace");
        let namespace = parse_with(&[("entities/reserved.toml", &entity)]).unwrap();
        let errors = emit_rust(&namespace, &options()).unwrap_err();
        assert_eq!(
            errors[0].to_string(),
            format!("`{name}` cannot be a generated struct name")
        );
    }
}

#[test]
fn rejects_names_that_collide_in_rust() {
    let property_set = "format = 1\n[property_set]\nname = \"Pset_Storey\"\napplies_to = \
                        [\"Storey\"]\n\n[[property]]\nname = \"IsExternal\"\nvalue = \
                        \"bool\"\n\n[[property]]\nname = \"ISExternal\"\nvalue = \"bool\"\n";
    let clashing_entity = STOREY
        .replace("Storey", "PsetStorey")
        .replace("IfcBuildingPsetStorey", "IfcSpace");
    let namespace = parse_with(&[
        ("property-sets/storey.toml", property_set),
        ("entities/pset.toml", &clashing_entity),
    ])
    .unwrap();
    let messages: Vec<_> = emit_rust(&namespace, &options())
        .unwrap_err()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        messages,
        [
            "two generated structs are named `PsetStorey`",
            "`ISExternal` in `PsetStorey` gives the same Rust field name as another one",
        ]
    );
}

/// A namespace directory under the system temporary directory, removed on drop.
struct TempNamespace(PathBuf);

impl TempNamespace {
    fn new(name: &str, files: &[(&str, &str)]) -> std::io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("schema-gen-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let namespace = Self(dir);
        for (path, contents) in files {
            let path = namespace.0.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, contents)?;
        }
        Ok(namespace)
    }
}

impl Drop for TempNamespace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn loading_needs_a_namespace_file() {
    let dir = TempNamespace::new("empty", &[]).unwrap();
    let error = load_dir(&dir.0).unwrap_err();
    assert!(matches!(error.kind, ErrorKind::Io(_)), "{error}");
    assert!(error.to_string().contains("namespace.toml"), "{error}");
}

#[test]
fn loading_rejects_stray_files() {
    for stray in ["entities/README.md", "entities/nested/wall.toml"] {
        let dir =
            TempNamespace::new("stray", &[("namespace.toml", NAMESPACE), (stray, "")]).unwrap();
        let error = load_dir(&dir.0).unwrap_err();
        assert!(
            error.to_string().contains("hold only *.toml files"),
            "{error}"
        );
    }
}

#[test]
fn loading_rejects_a_definition_path_that_is_not_a_directory() {
    let dir = TempNamespace::new(
        "not-dir",
        &[("namespace.toml", NAMESPACE), ("entities", "")],
    )
    .unwrap();
    let error = load_dir(&dir.0).unwrap_err();
    assert_eq!(
        error.to_string(),
        "entities: must be a directory of *.toml files"
    );
}

#[test]
fn loading_skips_absent_definition_directories() {
    let dir = TempNamespace::new("absent", &[("namespace.toml", NAMESPACE)]).unwrap();
    assert_eq!(load_dir(&dir.0).unwrap().len(), 1);
}

#[test]
fn loading_rejects_oversized_files() {
    let huge = "#".repeat(MAX_FILE_SIZE + 1);
    let dir = TempNamespace::new(
        "huge",
        &[("namespace.toml", NAMESPACE), ("entities/wall.toml", &huge)],
    )
    .unwrap();
    let error = load_dir(&dir.0).unwrap_err();
    assert_eq!(error.file.as_deref(), Some("entities/wall.toml"));
}

#[test]
fn generation_reports_loading_and_parsing_errors() {
    let dir = TempNamespace::new("generate", &[("namespace.toml", "format = 9\n")]).unwrap();
    let errors = generate_rust(&dir.0, &options()).unwrap_err();
    assert_eq!(errors[0].file.as_deref(), Some("namespace.toml"));

    let missing = dir.0.join("missing");
    let errors = generate_rust(&missing, &options()).unwrap_err();
    assert!(matches!(errors[0].kind, ErrorKind::Io(_)));
}

mod hostile_input {
    use proptest::prelude::*;

    use super::{parse_with, wall};

    proptest! {
        #![proptest_config(ProptestConfig {
            rng_seed: prop::test_runner::RngSeed::Fixed(0x5C4E_3A04),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        /// Arbitrary text never makes reading panic; it is accepted or reported.
        #[test]
        fn arbitrary_text_never_panics(text in ".{0,256}") {
            let _ = parse_with(&[("entities/x.toml", &text)]);
        }

        /// Plausible definitions with arbitrary values never panic either.
        #[test]
        fn arbitrary_attributes_never_panic(
            name in "[a-zA-Z_ ]{0,12}",
            value in "(list:){0,6}(bool|integer|real|text|ref:[A-Za-z]{0,6}|quantity:[a-z_]{0,8}|enum:X)",
            default in prop_oneof![
                Just(String::new()),
                Just("default = true".to_owned()),
                any::<i64>().prop_map(|n| format!("default = {n}")),
                any::<f64>().prop_map(|x| format!("default = {x:?}")),
                Just("default = [1.0, [2.0]]".to_owned()),
                Just("default = nan".to_owned()),
            ],
            on in "(instance|type|type_overridable)",
        ) {
            let attributes = format!(
                "\n[[attribute]]\nname = {name:?}\nvalue = {value:?}\non = {on:?}\n{default}\n"
            );
            let _ = parse_with(&[("entities/wall.toml", &wall(&attributes))]);
        }
    }
}
