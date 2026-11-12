//! A schema, commands and generators shared by the tests: walls with a type, a height, a
//! thickness and a finish, whose body is a box.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use core_model::{AttributeValue, Component, EntityId, LibraryRef, Model};
use core_txn::{ChangeEvent, Document, ModelCommand, Origin};
use geom::{Edge, FaceInput, Point2, Point3, Profile, Solid, Vector3};
use schema::{
    Attribute, Cardinality, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence,
    QuantityKind, Representation, Role, RoleTarget, TypeDef, Value, ValueType,
};

use crate::{Generated, GeneratorError, Generators, Scene, SceneUpdate, View};

fn attribute(name: &str, value: ValueType, presence: Presence, placement: Placement) -> Attribute {
    Attribute {
        name: name.into(),
        value,
        presence,
        placement,
        doc: None,
        sourced: false,
    }
}

fn entity(name: &str, kind: EntityKind, attributes: Vec<Attribute>) -> Entity {
    Entity {
        name: name.into(),
        kind,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: Some(IfcMapping {
            entity: format!("Ifc{name}"),
            predefined_type: None,
            object_type: None,
            type_entity: None,
        }),
        attributes,
        roles: Vec::new(),
        representations: Vec::new(),
    }
}

fn representation(context: &str, lod: u16, generator: &str, depends_on: &[&str]) -> Representation {
    Representation {
        lod,
        context: context.into(),
        generator: generator.into(),
        depends_on: depends_on.iter().map(|name| (*name).into()).collect(),
    }
}

/// The containment of elements in spatial structure elements.
fn containment() -> Entity {
    Entity {
        roles: vec![
            Role {
                name: "structure".into(),
                targets: vec![RoleTarget::Kind(EntityKind::Spatial)],
                cardinality: Cardinality::One,
                exclusive: false,
            },
            Role {
                name: "elements".into(),
                targets: vec![RoleTarget::Kind(EntityKind::Element)],
                cardinality: Cardinality::Many,
                exclusive: false,
            },
        ],
        ..entity("Containment", EntityKind::Relationship, Vec::new())
    }
}

/// Walls with a type, whose body reads their height, thickness and finish; columns with several
/// representations; finishes, which have no geometry; storeys, and their containment of walls.
pub fn namespace() -> Namespace {
    let length = || ValueType::Quantity(QuantityKind::Length);
    let wall = Entity {
        type_def: Some(TypeDef {
            name: "WallType".into(),
            doc: None,
        }),
        representations: vec![representation(
            "body",
            100,
            "wall::body",
            &["height", "thickness", "finish"],
        )],
        ..entity(
            "Wall",
            EntityKind::Element,
            vec![
                attribute(
                    "name",
                    ValueType::Text,
                    Presence::Optional,
                    Placement::Instance,
                ),
                attribute(
                    "height",
                    length(),
                    Presence::Default(Value::Real(2.5)),
                    Placement::TypeOverridable,
                ),
                attribute(
                    "thickness",
                    length(),
                    Presence::Default(Value::Real(0.2)),
                    Placement::Type,
                ),
                attribute(
                    "finish",
                    ValueType::Ref("Finish".into()),
                    Presence::Optional,
                    Placement::Instance,
                ),
            ],
        )
    };
    let column = Entity {
        representations: vec![
            representation("body", 100, "column::coarse", &["height"]),
            representation("body", 300, "column::fine", &["height"]),
            representation("axis", 100, "column::axis", &["height"]),
        ],
        ..entity(
            "Column",
            EntityKind::Element,
            vec![attribute(
                "height",
                length(),
                Presence::Default(Value::Real(3.0)),
                Placement::Instance,
            )],
        )
    };
    let finish = entity(
        "Finish",
        EntityKind::Element,
        vec![attribute(
            "offset",
            length(),
            Presence::Default(Value::Real(0.0)),
            Placement::Instance,
        )],
    );
    let storey = entity(
        "Storey",
        EntityKind::Spatial,
        vec![attribute(
            "elevation",
            length(),
            Presence::Optional,
            Placement::Instance,
        )],
    );
    let containment = containment();
    Namespace {
        name: "core".into(),
        version: 1,
        entities: [wall, column, finish, storey, containment]
            .into_iter()
            .map(|entity| (entity.name.clone(), entity))
            .collect(),
        property_sets: BTreeMap::new(),
        enumerations: BTreeMap::new(),
    }
}

/// The generator invocations so far, as the path and the element, in order.
#[derive(Clone, Default)]
pub struct Calls(Rc<RefCell<Vec<(String, EntityId)>>>);

impl Calls {
    fn push(&self, path: &str, element: EntityId) {
        self.0.borrow_mut().push((path.into(), element));
    }

    /// The invocations since the last call, in order.
    pub fn take(&self) -> Vec<(String, EntityId)> {
        std::mem::take(&mut self.0.borrow_mut())
    }

    /// The elements generated since the last call, sorted.
    pub fn elements(&self) -> Vec<EntityId> {
        let mut elements: Vec<EntityId> = self.take().into_iter().map(|(_, id)| id).collect();
        elements.sort();
        elements
    }
}

/// The finish that fine columns read, which nothing refers to.
pub const GLOBAL_FINISH: EntityId = EntityId::new(99);

pub fn id(raw: u128) -> EntityId {
    EntityId::new(raw)
}

pub fn ids<const N: usize>(raws: [u128; N]) -> BTreeSet<EntityId> {
    raws.into_iter().map(id).collect()
}

fn real(model: &Model, element: EntityId, attribute: &str) -> Result<f64, GeneratorError> {
    match model.get(element, attribute) {
        Ok(Some(AttributeValue::Real(value))) => Ok(value),
        _ => Err(GeneratorError::MissingInput),
    }
}

/// A prism of `height` over a rectangle `width` by `depth`.
pub fn prism(width: f64, depth: f64, height: f64) -> Result<Solid, GeneratorError> {
    let corners = [
        Point2::new(0.0, 0.0),
        Point2::new(width, 0.0),
        Point2::new(width, depth),
        Point2::new(0.0, depth),
    ];
    let edges = (0..corners.len())
        .map(|i| Edge::Line {
            start: corners[i],
            end: corners[(i + 1) % corners.len()],
        })
        .collect();
    let profile = Profile::new(edges, Vec::new())?;
    Ok(Solid::extrude(&profile, Vector3::new(0.0, 0.0, height))?)
}

/// A valid solid with a face that cannot be triangulated: a tube through a unit cube's top and
/// bottom faces, its holes outside them, which validation does not catch.
pub fn untriangulable_solid() -> Solid {
    let cube = prism(1.0, 1.0, 1.0).expect("a cube");
    let mut vertices = cube.vertices().to_vec();
    let mut faces = cube.to_faces();
    let cap = |vertices: &[Point3], high: bool| {
        faces_position(&cube.to_faces(), vertices, high).expect("a cap")
    };
    let (top, bottom) = (cap(&vertices, true), cap(&vertices, false));
    let first = vertices.len();
    for z in [1.0, 0.0] {
        for (x, y) in [(3.0, 0.0), (3.0, 0.5), (3.5, 0.5), (3.5, 0.0)] {
            vertices.push(Point3::new(x, y, z));
        }
    }
    faces[top].holes.push((first..first + 4).collect());
    faces[bottom]
        .holes
        .push((first + 4..first + 8).rev().collect());
    for i in 0..4 {
        let (a, b) = (first + i, first + (i + 1) % 4);
        faces.push(FaceInput {
            outer: vec![b, a, a + 4, b + 4],
            holes: Vec::new(),
        });
    }
    Solid::from_faces(vertices, &faces).expect("validation passes such a face")
}

fn faces_position(faces: &[FaceInput], vertices: &[Point3], high: bool) -> Option<usize> {
    faces
        .iter()
        .position(|face| face.outer.iter().all(|&v| (vertices[v].z > 0.5) == high))
}

/// The generators of [`namespace`], logging into `calls`. A wall named `unmeshable` has a solid
/// that cannot be tessellated.
pub fn generators(calls: &Calls) -> Generators {
    let mut generators = Generators::new();
    let log = calls.clone();
    generators.register(
        "wall::body",
        move |model: &Model, wall: EntityId| -> Result<Generated, GeneratorError> {
            log.push("wall::body", wall);
            let (offset, inputs) = match model.get(wall, "finish") {
                Ok(Some(AttributeValue::Ref(finish))) => {
                    (real(model, finish, "offset")?, BTreeSet::from([finish]))
                }
                _ => (0.0, BTreeSet::new()),
            };
            if model.get(wall, "name") == Ok(Some(AttributeValue::Text("unmeshable".into()))) {
                return Ok(Generated {
                    solids: vec![untriangulable_solid()],
                    inputs,
                });
            }
            let thickness = real(model, wall, "thickness")? + offset;
            let solid = prism(4.0, thickness, real(model, wall, "height")?)?;
            Ok(Generated {
                solids: vec![solid],
                inputs,
            })
        },
    );
    for (path, height) in [
        ("column::coarse", 1.0),
        ("column::fine", 2.0),
        ("column::axis", 3.0),
    ] {
        let log = calls.clone();
        generators.register(
            path,
            move |model: &Model, column: EntityId| -> Result<Generated, GeneratorError> {
                log.push(path, column);
                // The fine column reads a finish that no attribute of it refers to, and says so.
                let (extra, inputs) = match (path, model.get(GLOBAL_FINISH, "offset")) {
                    ("column::fine", Ok(Some(AttributeValue::Real(offset)))) => {
                        (offset, BTreeSet::from([GLOBAL_FINISH]))
                    }
                    ("column::fine", _) => (0.0, BTreeSet::from([GLOBAL_FINISH])),
                    _ => (0.0, BTreeSet::new()),
                };
                Ok(Generated {
                    solids: vec![prism(0.5, 0.5, height + extra)?],
                    inputs,
                })
            },
        );
    }
    generators
}

/// A document of [`namespace`], a scene over it, and the generator log.
pub struct Harness {
    pub doc: Document<ModelCommand>,
    pub scene: Scene,
    pub calls: Calls,
}

impl Harness {
    pub fn new() -> Self {
        Self::viewing(View::new("body", 100))
    }

    pub fn viewing(view: View) -> Self {
        let calls = Calls::default();
        Self {
            doc: Document::new(Model::new(&namespace()).expect("a valid schema")),
            scene: Scene::new(generators(&calls), view),
            calls,
        }
    }

    /// Runs `command` without telling the scene.
    pub fn execute(&mut self, command: ModelCommand) -> ChangeEvent {
        self.doc
            .execute(command, Origin::Ui)
            .expect("the command applies")
    }

    /// Builds the scene from scratch, then forgets the generator calls.
    pub fn rebuild(&mut self) -> SceneUpdate {
        let update = self.scene.rebuild(self.doc.model());
        self.calls.take();
        update
    }

    /// Runs `command`, tells the scene and updates it.
    pub fn run(&mut self, command: ModelCommand) -> SceneUpdate {
        let event = self.execute(command);
        self.sync(&event)
    }

    pub fn undo(&mut self) -> SceneUpdate {
        let event = self.doc.undo().expect("something to undo");
        self.sync(&event)
    }

    pub fn redo(&mut self) -> SceneUpdate {
        let event = self.doc.redo().expect("something to redo");
        self.sync(&event)
    }

    fn sync(&mut self, event: &ChangeEvent) -> SceneUpdate {
        self.scene.apply(event, self.doc.model());
        self.scene.update(self.doc.model())
    }
}

pub fn create(raw: u128, entity: &str, values: Vec<(&str, AttributeValue)>) -> ModelCommand {
    ModelCommand::Create {
        id: id(raw),
        entity: entity.into(),
        values: values
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    }
}

pub fn set(raw: u128, attribute: &str, value: Option<AttributeValue>) -> ModelCommand {
    ModelCommand::Set {
        id: id(raw),
        attribute: attribute.into(),
        value,
    }
}

pub fn set_real(raw: u128, attribute: &str, value: f64) -> ModelCommand {
    set(raw, attribute, Some(AttributeValue::Real(value)))
}

pub fn delete(raw: u128) -> ModelCommand {
    ModelCommand::Delete { id: id(raw) }
}

pub fn contain(raw: u128, storey: u128, walls: &[u128]) -> ModelCommand {
    ModelCommand::Relate {
        id: id(raw),
        entity: "Containment".into(),
        roles: vec![
            ("structure".into(), vec![id(storey)]),
            ("elements".into(), walls.iter().copied().map(id).collect()),
        ],
        values: Vec::new(),
    }
}

/// A wall component whose types are 0.2 m thick.
pub fn component(raw: u128) -> ModelCommand {
    ModelCommand::CreateComponent {
        id: id(raw),
        component: Component {
            entity: "Wall".into(),
            library: LibraryRef {
                id: "acme/partition".into(),
                version: 1,
            },
            defaults: vec![("thickness".into(), AttributeValue::Real(0.2))],
        },
    }
}

pub fn wall_type(raw: u128, component: u128) -> ModelCommand {
    ModelCommand::CreateType {
        id: id(raw),
        component: id(component),
        values: Vec::new(),
    }
}

pub fn assign(instance: u128, wall_type: Option<u128>) -> ModelCommand {
    ModelCommand::AssignType {
        instance: id(instance),
        type_id: wall_type.map(id),
    }
}

pub fn add_participant(relationship: u128, participant: u128) -> ModelCommand {
    ModelCommand::AddParticipant {
        relationship: id(relationship),
        role: "elements".into(),
        participant: id(participant),
    }
}

pub fn remove_participant(relationship: u128, participant: u128) -> ModelCommand {
    ModelCommand::RemoveParticipant {
        relationship: id(relationship),
        role: "elements".into(),
        participant: id(participant),
    }
}

pub fn batch(commands: Vec<ModelCommand>) -> ModelCommand {
    ModelCommand::Batch { commands }
}

/// Creates walls `1..=count`.
pub fn walls(count: u128) -> ModelCommand {
    batch(
        (1..=count)
            .map(|raw| create(raw, "Wall", Vec::new()))
            .collect(),
    )
}

/// The debug description of a new scene over [`generators`].
pub fn calls_debug() -> String {
    format!(
        "{:?}",
        Scene::new(generators(&Calls::default()), View::new("body", 100))
    )
}
