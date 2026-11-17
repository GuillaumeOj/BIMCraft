# 0027. Clipping and visibility in the viewport

- Status: Accepted
- Requirements: VP-03 (also VP-01, VP-02, VP-04, GEO-07, UI-06)
- Issues: [#72](https://github.com/GuillaumeOj/BIMCraft/issues/72), epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67)

## Context

Users cut the model with a section box and clipping planes to see inside it, and hide or isolate elements to read a crowded model. These are view state, not model state: they are lost when the window closes, and no command of the core is involved. The cut must work at any distance from the origin ([ADR 0020](0020-camera-relative-rendering.md)), must not slow the orbit of a large model ([ADR 0025](0025-instancing-culling-and-level-of-detail.md)), and what is hidden or cut away must never be picked or snapped to ([ADR 0026](0026-viewport-in-webassembly-with-a-binary-scene-channel.md): picking and snapping are CPU work in the viewport).

## Decision

**One list of planes, removed side first.** A plane is a point and a normal; it removes what lies on the side the normal points to. The section box is six planes (each removing what is outside the box), the user adds up to six more, so the renderer clips with at most twelve (`MAX_CLIP_PLANES`). A plane added on a picked face has the face's normal turned to the eye, so it removes what stands in front of the face.

**Clipping in the fragment shader, camera-relative.** Planes are kept in world coordinates in `f64`. Each frame the CPU sends, for each plane, its normal in the view's axes and the signed distance of the eye from it, computed in `f64` before one rounding to `f32` (the frame uniform grows to 320 bytes). The vertex stage passes the camera-space position; the fragment stage discards what a plane removes. Nothing in the shader is a world coordinate, so a cut at 10⁷ m is as exact as one at the origin.

**Cut faces are the far faces seen through the cut.** Solids are not capped. A back face visible while a clipping plane is active is the inside of a cut solid, and the shader paints it flat in the theme's cut colour (`vp-cut`, a solid fill per the design); with `CutFaces::None` it is drawn as any back face. A back face is told by its winding (`front_facing`): meshes turn counterclockwise seen from outside, as geom tessellates them (GEO-05), and the renderer's test meshes check they do. Ghosts keep their own translucent colour. Models with inconsistently oriented faces show some tinted back faces while a cut is active, which a capped section would not; capping needs a stencil pass and is left to a later feature.

**Culling takes the planes.** The frustum gains the clipping planes after its own, so a cluster of the object hierarchy entirely on the removed side is skipped with its whole subtree: a fully clipped object costs no instance record, no draw call and no occlusion query. Culling is conservative (the same margin as the view planes), so no visible pixel is lost.

**Visibility is a display state, decided by the viewport.** Three inputs hide elements, in this order: categories, disciplines, and a set of elements ("hide selection"). An isolation (by categories, disciplines or elements) then keeps only what it names among what is not hidden; isolating something first shows it. The viewport recomputes which elements are hidden when a command or a scene delta changes the inputs, and sets `DisplayState::Hidden` on the changes only: a hidden object stays in the renderer (showing it is one call) but a frame skips it, and so do pick queries. Hidden elements leave the selection and the hover. Esc ends an isolation before it clears the selection. Categories and disciplines are the names of the scene's string tables, interned by the viewport.

**Picking and snapping share one gate.** The `Pickable` test of the interaction accepts an element if it is not hidden and a point if no plane removes it (with a micrometre of tolerance, so a face a plane was picked on stays pickable). Both picking and snapping go through it, so hidden or cut-away geometry is never offered.

**The section box has face handles.** Six handles, one per face, sit at the face centres, are drawn as overlay markers and are taken from a pointer within 12 px. Dragging one moves its face along its axis to where the pointer ray passes nearest the axis line, in `f64` relative to the eye; with snapping on it takes the coordinate of a snapped model point (an endpoint, a midpoint or an edge), with Shift it rounds to 100 mm. A face cannot cross its opposite one. A drag cut short (Esc, the pointer leaving, the focus lost) puts the box back. The section box and the planes are drawn through the overlay: the twelve edges in the selection colour, a dashed square and an arrow per plane.

**What the UI sees.** `view_state` carries the lists of categories and disciplines with their counts, hidden and isolated flags, whether an isolation is on, how many elements "hide selection" hid, the section box (or `null` before it was first turned on, fitted or typed) and the planes. It is built and sent only when something it shows changed, not on every pointer move. Announcements are keys with raw values (`visibility.hidden` and `visibility.isolated` with `count`, `clip_plane.added` and `clip_plane.removed` with `count`, `clip_plane.flipped` with `index`, `section_box.face_moved` with `face` and `value`).

## Consequences

- The shader pays a loop over the active planes per fragment, nothing when there are none; the CPU pays one `f64` dot product per plane and node of the hierarchy while one is active.
- Visibility changes cost one pass over the elements and one renderer call per element that changed; a per-category toggle on 100 000 elements is one such pass, not one per frame.
- A tinted cut is an approximation of a section (no real cap, no edges), as the design asks for no edges in this feature.
- Typed extents that are not finite or whose minimum is not below the maximum are ignored; the UI validates and explains them.
