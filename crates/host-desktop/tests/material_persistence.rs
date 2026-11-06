//! Materials survive saving, closing and reopening the project file with every value and source
//! (MAT-01, ADR 0019), on the core schema the application embeds.

use std::collections::BTreeMap;
use std::path::Path;

use core_model::{EntityId, Model, Source};
use core_txn::{Document, ModelCommand, Origin};
use host_desktop::core_namespace;
use material::{Category, LawKind, Material, Property, Sourced};
use persist::{Connection, Schema, open_model, open_project, save_model};
use proptest::prelude::*;
use schema::Namespace;
use tempfile::TempDir;

#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn sourced(value: f64, document: &str, clause: &str) -> Sourced<f64> {
    Sourced::new(value, Source::new(document, clause).unwrap())
}

/// C30/37, S355 and B500B, with values from three documents.
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn materials() -> Vec<Material> {
    let ec2 = |value, clause| sourced(value, "EN 1992-1-1:2004", clause);
    let ec3 = |value, clause| sourced(value, "EN 1993-1-1:2005", clause);
    let concrete = Material::new(
        "C30/37",
        Category::Concrete,
        LawKind::ConcreteParabolaRectangle,
        Some("EN 1992-1-1:2004"),
        BTreeMap::from([
            (
                Property::CharacteristicCompressiveStrength,
                ec2(30e6, "Table 3.1"),
            ),
            (Property::CharacteristicCubeStrength, ec2(37e6, "Table 3.1")),
            (Property::MeanCompressiveStrength, ec2(38e6, "Table 3.1")),
            (Property::MeanTensileStrength, ec2(2.9e6, "Table 3.1")),
            (Property::YoungModulus, ec2(33e9, "Table 3.1")),
            (Property::PoissonRatio, ec2(0.2, "3.1.3(4)")),
            (Property::StrainAtPeakStress, ec2(0.002, "Table 3.1")),
            (
                Property::UltimateCompressiveStrain,
                ec2(0.0035, "Table 3.1"),
            ),
            (Property::ParabolaExponent, ec2(2.0, "Table 3.1")),
            (
                Property::MassDensity,
                sourced(2500.0, "EN 1991-1-1:2002", "Table A.1"),
            ),
        ]),
    );
    let steel = Material::new(
        "S355",
        Category::Steel,
        LawKind::SteelBilinear,
        Some("EN 1993-1-1:2005"),
        BTreeMap::from([
            (Property::YieldStrength, ec3(355e6, "Table 3.1")),
            (Property::UltimateStrength, ec3(490e6, "Table 3.1")),
            (Property::YoungModulus, ec3(210e9, "3.2.6(1)")),
            (Property::ShearModulus, ec3(81e9, "3.2.6(1)")),
            (Property::PoissonRatio, ec3(0.3, "3.2.6(1)")),
            (
                Property::ThermalExpansionCoefficient,
                ec3(12e-6, "3.2.6(1)"),
            ),
        ]),
    );
    let reinforcement = Material::new(
        "B500B",
        Category::Reinforcement,
        LawKind::SteelBilinear,
        Some("EN 1992-1-1:2004"),
        BTreeMap::from([
            (Property::YieldStrength, ec2(500e6, "Table C.1")),
            (Property::YoungModulus, ec2(200e9, "3.2.7(4)")),
            (Property::HardeningRatio, ec2(1.08, "Table C.1")),
            (Property::UltimateStrain, ec2(0.05, "Table C.1")),
        ]),
    );
    vec![concrete.unwrap(), steel.unwrap(), reinforcement.unwrap()]
}

/// Stores `materials` as objects 1, 2, … through undoable commands.
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn model_with(namespace: &Namespace, materials: &[Material]) -> Model {
    let mut document = Document::<ModelCommand>::new(Model::new(namespace).unwrap());
    for (raw, material) in (1..).zip(materials) {
        let command = material::commands(EntityId::new(raw), material);
        document.execute(command, Origin::Ui).unwrap();
    }
    document.model().clone()
}

/// Saves `model` to a new project file at `path` and closes it.
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn save(path: &Path, namespace: &Namespace, model: &Model) {
    let mut conn = Connection::open(path).unwrap();
    open_project(&mut conn).unwrap();
    save_model(&mut conn, &Schema::new(namespace), model.store()).unwrap();
    conn.close().unwrap();
}

/// Opens the project file at `path` again and rebuilds its model.
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn reopen(path: &Path, namespace: &Namespace) -> Model {
    let mut conn = Connection::open(path).unwrap();
    open_project(&mut conn).unwrap();
    let store = open_model(&mut conn, &Schema::new(namespace)).unwrap();
    Model::from_store(namespace, &store).unwrap()
}

/// Checks that `reopened` holds the same objects as `saved`, attribute by attribute (values and
/// sources), and reads the same materials back.
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn assert_same(saved: &Model, reopened: &Model, materials: &[Material]) {
    assert_eq!(reopened.len(), saved.len());
    for (raw, material) in (1..).zip(materials) {
        let id = EntityId::new(raw);
        assert_eq!(reopened.store().entity(id), saved.store().entity(id));
        assert_eq!(material::read(reopened, id).as_ref(), Ok(material));
    }
}

#[test]
fn materials_keep_every_value_and_source_after_reopening() {
    let namespace = core_namespace().unwrap();
    let materials = materials();
    let saved = model_with(&namespace, &materials);
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("materials.bim");

    save(&path, &namespace, &saved);
    let reopened = reopen(&path, &namespace);

    assert_same(&saved, &reopened, &materials);
    let concrete = material::read(&reopened, EntityId::new(1)).unwrap();
    let density = concrete.property(Property::MassDensity).unwrap();
    assert_eq!(density.source.document(), "EN 1991-1-1:2002");
    assert_eq!(density.source.clause(), "Table A.1");
}

#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn source() -> impl Strategy<Value = Source> {
    (
        "EN 199[0-9]-1-1:20[0-2][0-9]",
        "(Table )?[1-9]\\.[0-9]{1,2}(\\([1-9]\\))?",
    )
        .prop_map(|(document, clause)| Source::new(&document, &clause).unwrap())
}

/// A value `property` accepts: a ratio below one for strains, Poisson's ratio below one half,
/// at least one for the hardening ratio, else a positive number.
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn value(property: Property) -> BoxedStrategy<f64> {
    match property {
        Property::PoissonRatio => (0.0..0.5).boxed(),
        Property::HardeningRatio => (1.0..2.0).boxed(),
        Property::StrainAtPeakStress
        | Property::UltimateCompressiveStrain
        | Property::BilinearStrainAtPeakStress
        | Property::BilinearUltimateCompressiveStrain
        | Property::UltimateStrain => (1e-4..0.5).boxed(),
        _ => (1e-3..1e12).boxed(),
    }
}

/// A linear elastic material of any category, giving its modulus, the properties its category
/// requires and any others of the category, each with a source.
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn material() -> impl Strategy<Value = Option<Material>> {
    let properties = Property::ALL.map(|property| {
        (value(property), source(), any::<bool>())
            .prop_map(move |(value, source, given)| (property, Sourced::new(value, source), given))
    });
    let category = proptest::sample::select(Category::ALL.to_vec());
    ("[A-Z][A-Za-z0-9/ ]{0,30}", category, properties).prop_map(|(name, category, properties)| {
        let properties = properties
            .into_iter()
            .filter(|(property, _, given)| {
                let needed =
                    *property == Property::YoungModulus || category.required().contains(property);
                category.gives(*property) && (*given || needed)
            })
            .map(|(property, value, _)| (property, value))
            .collect();
        Material::new(&name, category, LawKind::LinearElastic, None, properties).ok()
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x00C0_FFEE),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn any_valid_material_survives_a_save_and_reopen(
        materials in proptest::collection::vec(material(), 1..4),
    ) {
        let materials: Vec<Material> = materials.into_iter().flatten().collect();
        prop_assume!(!materials.is_empty());
        let namespace = core_namespace().unwrap();
        let saved = model_with(&namespace, &materials);
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("random.bim");
        save(&path, &namespace, &saved);
        let reopened = reopen(&path, &namespace);
        assert_same(&saved, &reopened, &materials);
    }
}
