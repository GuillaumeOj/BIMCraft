//! Where the viewport sends what it shows: the renderer, through [`SceneSink`], so every
//! decision of the viewport is tested natively against a recording fake.

use std::collections::BTreeMap;

use render::{
    Camera, Clip, ClipError, ClipPlane, CutFaces, DisplayState, MeshData, MeshError, MeshId,
    ObjectId, Overlay, OverlayError, Placement, RenderError, Renderer, SnapKind, Theme, Tone,
};

use crate::ElementId;

/// Why the renderer refused something.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SinkError {
    /// A mesh was refused.
    #[error(transparent)]
    Mesh(#[from] MeshError),
    /// An object, a state or an overlay was refused.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// An item of a preview was refused.
    #[error(transparent)]
    Overlay(#[from] OverlayError),
    /// The clipping planes were refused.
    #[error(transparent)]
    Clip(#[from] ClipError),
    /// The element has no object in the renderer.
    #[error("the element is not in the renderer")]
    UnknownElement,
    /// The mesh is not in the renderer.
    #[error("the mesh is not in the renderer")]
    UnknownMesh,
}

/// A line of a preview.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewLine {
    /// One end, in world metres.
    pub from: [f64; 3],
    /// The other end.
    pub to: [f64; 3],
    /// Its colour.
    pub tone: Tone,
}

/// The transient geometry the viewport draws over the model: rubber bands, snap markers, the
/// block being placed and ghosts of the moved elements. Nothing in it is model state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    /// Lines.
    pub lines: Vec<PreviewLine>,
    /// Snap markers.
    pub markers: Vec<([f64; 3], SnapKind)>,
    /// Translucent boxes, `(min, max)`.
    pub boxes: Vec<([f64; 3], [f64; 3])>,
    /// Ghosts of elements, translated.
    pub ghosts: Vec<(ElementId, [f64; 3])>,
}

impl Preview {
    /// Whether it shows nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
            && self.markers.is_empty()
            && self.boxes.is_empty()
            && self.ghosts.is_empty()
    }
}

/// What the viewport needs from a renderer. Meshes are named by their id on the scene channel
/// and objects by their element, so the viewport never handles the renderer's own ids.
pub trait SceneSink {
    /// Uploads a mesh.
    ///
    /// # Errors
    ///
    /// When the renderer refuses it.
    fn add_mesh(&mut self, key: u64, mesh: &MeshData) -> Result<(), SinkError>;

    /// Removes a mesh, which no object places any more.
    ///
    /// # Errors
    ///
    /// When the renderer does not hold it.
    fn remove_mesh(&mut self, key: u64) -> Result<(), SinkError>;

    /// Places the mesh `mesh` for `element`.
    ///
    /// # Errors
    ///
    /// When the renderer refuses it.
    fn add_object(
        &mut self,
        element: ElementId,
        mesh: u64,
        placement: Placement,
    ) -> Result<(), SinkError>;

    /// Moves the object of `element`.
    ///
    /// # Errors
    ///
    /// When the renderer refuses it.
    fn set_placement(&mut self, element: ElementId, placement: Placement) -> Result<(), SinkError>;

    /// Removes the object of `element`.
    ///
    /// # Errors
    ///
    /// When the renderer does not hold it.
    fn remove_object(&mut self, element: ElementId) -> Result<(), SinkError>;

    /// Sets how the object of `element` is drawn.
    ///
    /// # Errors
    ///
    /// When the renderer does not hold it.
    fn set_display_state(
        &mut self,
        element: ElementId,
        state: DisplayState,
    ) -> Result<(), SinkError>;

    /// Replaces the preview; an empty one clears it. A refused preview leaves the previous one.
    ///
    /// # Errors
    ///
    /// When the renderer refuses it, notably [`RenderError::OverlayTooLarge`].
    fn set_preview(&mut self, preview: &Preview) -> Result<(), SinkError>;

    /// Sets the colours of the view.
    fn set_theme(&mut self, theme: Theme);

    /// Sets the planes frames clip with and how the cut shows.
    ///
    /// # Errors
    ///
    /// When the renderer refuses the planes.
    fn set_clip(&mut self, planes: &[ClipPlane], cut: CutFaces) -> Result<(), SinkError>;

    /// Calls `found` with every element whose bounds meet the view through `rect`,
    /// `[left, top, right, bottom]` in pixels of a target of `size`.
    ///
    /// # Errors
    ///
    /// When the camera or the rectangle is invalid.
    fn elements_in_rect(
        &mut self,
        camera: &Camera,
        size: [u32; 2],
        rect: [f64; 4],
        found: &mut dyn FnMut(ElementId),
    ) -> Result<(), SinkError>;
}

/// The neutral colour of the model's objects (linear RGBA): the theme has no model colour yet.
const OBJECT_COLOR: [f32; 4] = [0.6, 0.6, 0.6, 1.0];

/// The width of the lines of a preview, in pixels (board 3.28: 2 px).
const LINE_WIDTH: f32 = 2.0;

/// A [`Renderer`] as a [`SceneSink`]: it maps the scene's mesh ids and the elements to the
/// renderer's ids.
pub struct RendererSink {
    renderer: Renderer,
    meshes: BTreeMap<u64, MeshId>,
    objects: BTreeMap<ElementId, ObjectId>,
    elements: BTreeMap<ObjectId, ElementId>,
    overlay: Overlay,
}

impl RendererSink {
    /// Wraps `renderer`, which holds nothing yet.
    #[must_use]
    pub fn new(renderer: Renderer) -> Self {
        Self {
            renderer,
            meshes: BTreeMap::new(),
            objects: BTreeMap::new(),
            elements: BTreeMap::new(),
            overlay: Overlay::new(),
        }
    }

    /// The renderer, to draw a frame.
    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    fn object(&self, element: ElementId) -> Result<ObjectId, SinkError> {
        self.objects
            .get(&element)
            .copied()
            .ok_or(SinkError::UnknownElement)
    }

    fn mesh(&self, key: u64) -> Result<MeshId, SinkError> {
        self.meshes.get(&key).copied().ok_or(SinkError::UnknownMesh)
    }

    fn build_overlay(&mut self, preview: &Preview) -> Result<(), SinkError> {
        self.overlay.clear();
        for line in &preview.lines {
            self.overlay
                .line(line.from, line.to, LINE_WIDTH, line.tone)?;
        }
        for &(at, kind) in &preview.markers {
            self.overlay.marker(at, kind)?;
        }
        for &(min, max) in &preview.boxes {
            self.overlay.translucent_box(min, max, Tone::Preview)?;
        }
        for &(element, translation) in &preview.ghosts {
            let object = self.object(element)?;
            self.overlay
                .ghost(object, Placement::from_translation(translation))?;
        }
        Ok(())
    }
}

impl SceneSink for RendererSink {
    fn add_mesh(&mut self, key: u64, mesh: &MeshData) -> Result<(), SinkError> {
        let id = self.renderer.add_mesh(mesh)?;
        if let Some(old) = self.meshes.insert(key, id) {
            self.renderer.remove_mesh(old)?;
        }
        Ok(())
    }

    fn remove_mesh(&mut self, key: u64) -> Result<(), SinkError> {
        // The viewport removes the objects of a mesh before the mesh.
        self.renderer.remove_mesh(self.mesh(key)?)?;
        self.meshes.remove(&key);
        Ok(())
    }

    fn add_object(
        &mut self,
        element: ElementId,
        mesh: u64,
        placement: Placement,
    ) -> Result<(), SinkError> {
        let mesh = self.mesh(mesh)?;
        let object = self.renderer.add_object(mesh, placement, OBJECT_COLOR)?;
        if let Some(old) = self.objects.insert(element, object) {
            self.elements.remove(&old);
            self.renderer.remove_object(old)?;
        }
        self.elements.insert(object, element);
        Ok(())
    }

    fn set_placement(&mut self, element: ElementId, placement: Placement) -> Result<(), SinkError> {
        let object = self.object(element)?;
        Ok(self.renderer.set_placement(object, placement)?)
    }

    fn remove_object(&mut self, element: ElementId) -> Result<(), SinkError> {
        let object = self.object(element)?;
        self.renderer.remove_object(object)?;
        self.objects.remove(&element);
        self.elements.remove(&object);
        Ok(())
    }

    fn set_display_state(
        &mut self,
        element: ElementId,
        state: DisplayState,
    ) -> Result<(), SinkError> {
        let object = self.object(element)?;
        Ok(self.renderer.set_display_state(object, state)?)
    }

    fn set_preview(&mut self, preview: &Preview) -> Result<(), SinkError> {
        if preview.is_empty() {
            self.renderer.clear_overlay();
            return Ok(());
        }
        self.build_overlay(preview)?;
        Ok(self.renderer.set_overlay(&self.overlay)?)
    }

    fn set_theme(&mut self, theme: Theme) {
        self.renderer.set_theme(theme);
    }

    fn set_clip(&mut self, planes: &[ClipPlane], cut: CutFaces) -> Result<(), SinkError> {
        self.renderer.set_clip(Clip::new(planes, cut)?);
        Ok(())
    }

    fn elements_in_rect(
        &mut self,
        camera: &Camera,
        size: [u32; 2],
        rect: [f64; 4],
        found: &mut dyn FnMut(ElementId),
    ) -> Result<(), SinkError> {
        let elements = &self.elements;
        self.renderer
            .objects_in_rect(camera, size, rect, |object| {
                if let Some(&element) = elements.get(&object) {
                    found(element);
                }
            })?;
        Ok(())
    }
}
