# Cut the model with a section box

Cut away part of the model in the 3D view to look inside it. A section box is a box around the part you want to keep: everything outside it is cut away, and the cut surfaces are tinted so you can read them. You can also cut with one or more clipping planes. Cutting only changes what you see; it never changes the model.

## Before you start

- The model has at least one element, for example a [block](place-a-block.md).
- The 3D view is running. The View panel on the right is where the settings are.

A few words used on this page:

- **Section box.** A box whose faces cut the model. What is outside it is not drawn and cannot be selected.
- **Clipping plane.** An endless flat surface that cuts away everything on one side of it. You can add up to six.
- **Cut face.** The surface that appears where the model is cut. It is tinted in orange so it never looks selected.

## Switch the section box on

1. Open the **View** tab of the right panel.
2. In the **Section box** section, tick **Active**. **Shortcut:** `Alt+B`.

**Result:** a box that fits the whole model, with a small margin, appears with a handle on each face. The panel tag reads "Section box active", and a message tells you how to move the faces.

## Move the faces of the box

There are two equivalent ways to move a face.

**By dragging.** Drag a handle of the box. Hold **Shift** to move in steps of 0.100 m. The status bar reads, for example, "Section box · 1 face moved".

**By typing.** In the panel, the **Min** and **Max** fields give the position of each face along X, Y and Z, in metres.

1. Click a field, or tab to it.
2. Type the new value in metres, for example `12.5` (use a comma in French).
3. Press **Enter** to apply it, or **Esc** to put the old value back. Leaving the field with **Tab** also applies it.

**Result:** the face moves and the view is cut at its new position.

A minimum must stay below its maximum. If it does not, the field turns red and says "Minimum must be below maximum"; nothing changes until you fix the value.

## Other settings

| Option | What it does |
| --- | --- |
| **Cut faces** | **Tinted** colours the cut surfaces; **None** leaves them open. |
| **Fit to selection** | Resizes the box around the selected elements. Select something first. |
| **Reset** | Puts the box back around the whole model. |
| **Turn off** | Switches the section box off (`Alt+B`). |

## Cut with clipping planes

1. In the **Clipping planes** section, choose **Add plane on face**. A message reads "Click a face to cut at it · Esc to cancel".
2. Click a face of the model. The plane goes through that point, facing you, so what lies between you and the face is cut away.

**Result:** the panel lists "Plane 1", with its direction (for example "Normal +X") and its position in metres. The status bar says how many planes are active.

For each plane in the list:

- **Flip** reverses the side that is cut away.
- **Remove** deletes the plane. With the focus on a plane's buttons, **Delete** removes it too.
- **Clear all** removes every plane. This cannot be undone with **Undo**: planes are part of the view, not of the model.

You can have six planes at most; at six, **Add plane on face** says "Maximum 6 planes".

## Tips

- Use the **Top**, **Front** and **Left** views from the view controls to read sections as plans and elevations.
- The section box and the planes are kept only while the window is open. They are not saved with the project yet.
- Hidden elements stay hidden inside a cut; see [Hide and isolate elements](hide-and-isolate-elements.md).

## Troubleshooting

**The section box is on but the model looks the same.**
The box is larger than the model, or it still has its starting size. Choose **Reset**, then move a face in.

**I cannot select an element.**
It is cut away. Elements outside the box or behind a plane are not drawn and cannot be selected. Move the face or remove the plane.

**A field turns red.**
The value is not a number, or the minimum is not below the maximum. Read the message under the fields, correct the value and press **Enter**.

**"Fit to selection" does nothing.**
It needs a selection. Select an element in the 3D view first.

**A cut surface shows stray tinted patches.**
Surfaces of some imported elements are not closed cleanly. The tint is then only approximate; the cut itself is correct.

## See also

- [Hide and isolate elements](hide-and-isolate-elements.md)
- [Navigate the 3D view](navigate-the-3d-view.md)
