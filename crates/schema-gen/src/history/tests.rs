use schema::{MigrationError, Step};

use super::*;
use crate::{parse_sources, snapshot};

/// A namespace at `version` with one storey entity holding `attributes` (TOML).
fn namespace(version: u32, attributes: &str) -> Namespace {
    let sources = [
        Source {
            path: "namespace.toml".into(),
            contents: format!("format = 1\n[namespace]\nname = \"core\"\nversion = {version}\n"),
        },
        Source {
            path: "entities/storey.toml".into(),
            contents: format!(
                "format = 1\n[entity]\nname = \"Storey\"\nkind = \"spatial\"\n[ifc]\nentity = \"IfcBuildingStorey\"\n{attributes}"
            ),
        },
    ];
    parse_sources(&sources).unwrap()
}

const V1: &str = "[[attribute]]\nname = \"label\"\nvalue = \"text\"\n";
const V2: &str = "[[attribute]]\nname = \"name\"\nvalue = \"text\"\nrequired = true\n";

fn source(path: &str, contents: &str) -> Source {
    Source {
        path: path.into(),
        contents: contents.into(),
    }
}

fn snapshots(namespaces: &[Namespace]) -> Vec<Source> {
    namespaces
        .iter()
        .map(|namespace| {
            source(
                &snapshot_path(namespace.version),
                &snapshot(namespace).unwrap(),
            )
        })
        .collect()
}

const RENAME: &str = r#"format = 1

[[step]]
op = "rename_attribute"
entity = "Storey"
old_name = "label"
new_name = "name"

[[step]]
op = "set_value"
entity = "Storey"
attribute = "name"
value = "Unnamed"
"#;

fn messages(errors: &[Error]) -> Vec<String> {
    errors.iter().map(ToString::to_string).collect()
}

#[test]
fn a_complete_history_resolves_every_migration() {
    let v1 = namespace(1, V1);
    let v2 = namespace(2, V2);
    let history = check_history(
        v2.clone(),
        &snapshots(&[v1, v2.clone()]),
        &[source("migrations/0002.toml", RENAME)],
    )
    .unwrap();
    assert_eq!(history.namespace, v2);
    assert_eq!(history.migrations.len(), 1);
    assert_eq!(history.migrations[0].version, 2);
    assert_eq!(history.migrations[0].steps.len(), 2);
}

#[test]
fn a_version_without_data_changes_needs_no_migration_file() {
    let v1 = namespace(1, V1);
    let v2 = namespace(2, V1);
    let history = check_history(v2.clone(), &snapshots(&[v1, v2]), &[]).unwrap();
    assert_eq!(history.migrations[0].steps, Vec::<Step>::new());
}

#[test]
fn a_schema_change_without_a_new_version_fails() {
    let released = namespace(1, V1);
    let changed = namespace(1, V2);
    let errors = check_history(changed, &snapshots(&[released]), &[]).unwrap_err();
    assert_eq!(
        messages(&errors),
        [
            "snapshots/0001.json: the definitions differ from this snapshot: raise the version in namespace.toml to 2 and describe the change in migrations/0002.toml"
        ]
    );
}

#[test]
fn a_schema_change_without_a_migration_fails() {
    let v1 = namespace(1, V1);
    let v2 = namespace(2, V2);
    let errors = check_history(v2.clone(), &snapshots(&[v1, v2]), &[]).unwrap_err();
    assert_eq!(
        errors[0].kind,
        ErrorKind::Migration(Box::new(MigrationError::Unfilled {
            entity: "Storey".into(),
            attribute: "name".into()
        }))
    );
    assert_eq!(errors[0].file.as_deref(), Some("migrations/0002.toml"));
}

#[test]
fn missing_and_unexpected_files_are_reported() {
    let v2 = namespace(2, V1);
    let errors = check_history(
        v2.clone(),
        &[
            source("snapshots/0005.json", "{}"),
            source("snapshots/1.json", "{}"),
        ],
        &[source("migrations/0001.toml", "")],
    )
    .unwrap_err();
    assert_eq!(
        messages(&errors),
        [
            "snapshots/0001.json: missing: snapshots of released versions are never removed",
            "snapshots/0002.json: missing: run `UPDATE_SNAPSHOTS=1 cargo test -p schema-gen` to write it",
            "snapshots/0005.json: unexpected file: the namespace is at version 2",
            "snapshots/1.json: unexpected file: the namespace is at version 2",
            "migrations/0001.toml: unexpected file: the namespace is at version 2",
        ]
    );
}

#[test]
fn unreadable_snapshots_are_reported() {
    let v1 = namespace(1, V1);
    let mut invalid = namespace(2, V1);
    invalid.entities.get_mut("Storey").unwrap().ifc = None;
    let errors = check_history(
        namespace(3, V1),
        &[
            source("snapshots/0001.json", "not json"),
            source("snapshots/0002.json", &snapshot(&invalid).unwrap()),
            source("snapshots/0003.json", &snapshot(&v1).unwrap()),
        ],
        &[],
    )
    .unwrap_err();
    let messages = messages(&errors);
    assert!(
        messages[0].starts_with("snapshots/0001.json: expected ident"),
        "{messages:?}"
    );
    assert!(
        messages[1].starts_with("snapshots/0002.json: the snapshot is not a valid schema: "),
        "{messages:?}"
    );
    assert_eq!(messages[2], "snapshots/0003.json: holds version 1, not 3");
    assert_eq!(messages.len(), 3);
}

#[test]
fn malformed_migration_files_are_reported() {
    let v1 = namespace(1, V1);
    let v2 = namespace(2, V1);
    let cases = [
        (
            "format = 1\n[[step]]\nop = \"rename\"\n",
            "unknown variant `rename`",
        ),
        (
            "format = 2\n",
            "format 2 is not supported; this version reads format 1",
        ),
        (
            "format = 1\n[[step]]\nop = \"rename_entity\"\nold_name = \"A\"\nnew_name = \"B\"\nextra = 1\n",
            "unknown field `extra`",
        ),
        (
            "format = 1\n[[step]]\nop = \"change_value\"\nentity = \"Storey\"\nattribute = \"label\"\nvalue = \"float\"\n",
            "unknown value type \"float\"",
        ),
        (
            "format = 1\n[[step]]\nop = \"set_value\"\nentity = \"Storey\"\nattribute = \"label\"\nvalue = { a = 1 }\n",
            "values must be booleans, numbers, text or lists of them",
        ),
        (
            "format = 1\n[[step]]\nop = \"custom\"\nentities = [\"Storey\"]\nfunction = \"f\"\n",
            "missing field `doc`",
        ),
    ];
    for (contents, expected) in cases {
        let errors = check_history(
            v2.clone(),
            &snapshots(&[v1.clone(), v2.clone()]),
            &[source("migrations/0002.toml", contents)],
        )
        .unwrap_err();
        let message = errors[0].to_string();
        assert!(message.starts_with("migrations/0002.toml:"), "{message}");
        assert!(message.contains(expected), "{message}");
    }
}

#[test]
fn declared_steps_are_read_with_their_defaults() {
    let source = source(
        "migrations/0002.toml",
        r#"format = 1

[[step]]
op = "rename_entity"
old_name = "Level"
new_name = "Storey"

[[step]]
op = "change_value"
entity = "Storey"
attribute = "height"
value = "quantity:length"

[[step]]
op = "custom"
entities = ["Storey"]
function = "migrations::v2::f"
doc = "Explains itself."
"#,
    );
    assert_eq!(
        parse_migration(&source).unwrap(),
        [
            DeclaredStep::RenameEntity {
                old_name: "Level".into(),
                new_name: "Storey".into()
            },
            DeclaredStep::ChangeValue {
                entity: "Storey".into(),
                attribute: "height".into(),
                value: "quantity:length".parse().unwrap(),
                factor: 1.0
            },
            DeclaredStep::Custom {
                entities: vec!["Storey".into()],
                function: "migrations::v2::f".into()
            },
        ]
    );
}
