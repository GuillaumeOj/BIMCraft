# Navigate the 3D view

The 3D view in the middle of the window shows your model as shaded volumes. You turn around it, slide it and zoom in and out with the mouse, a trackpad or the keyboard, and you jump to standard views such as Top or an isometric view.

## Before you start

- Nothing: the 3D view opens with the project. An empty project shows a grey background and the message "Nothing to see yet"; [place a block](place-a-block.md) to have something to look at.

A few words used on this page:

- **Orbit.** Turning the view around the model, as if you walked around it.
- **Pan.** Sliding the view sideways or up and down without turning it.
- **Perspective / parallel projection.** In perspective, far objects look smaller, as in a photograph. In parallel projection, they keep their size, as in a technical drawing.

## Move around with the mouse or trackpad

| To | Mouse | Trackpad |
| --- | --- | --- |
| Orbit | Right-click and drag | Option + drag (macOS) |
| Pan | Middle-click and drag, or Shift + drag | Two fingers |
| Zoom | Wheel, or Ctrl + wheel | Pinch |

The first time the view opens, a card titled **Moving around the model** lists these moves. **Got it** hides it until you restart BIMCraft; **Don't show again** hides it for good.

## Move around with the keyboard

1. Click the 3D view, or press **Tab** until the 3D view has a green outline. The view is named "3D view" for screen readers.
2. Use the keys below.
3. Press **Tab** to leave the view: it never keeps the keyboard.

| Key | What it does |
| --- | --- |
| Arrows | Orbit |
| Shift + arrows | Pan |
| `+` / `-` | Zoom in / out |
| `F` | Zoom fit: frames the whole model |
| Shift + `F` | Zoom to selection: frames the selected elements |
| `P` | Switch between perspective and parallel projection |

**Result:** the view moves at once; nothing in the model changes.

## Use the view controls

The view controls sit at the bottom right of the window, from left to right:

| Control | What it does |
| --- | --- |
| **Zoom fit** | Frames the whole model. **Shortcut:** `F`. Unavailable while the model is empty. |
| **Zoom to selection** | Frames the selected elements. **Shortcut:** Shift + `F`. Unavailable with nothing selected. |
| **View presets** | Opens a menu of standard views: Top, Front, Back, Left, Right, and four isometric views (Iso south-east, south-west, north-east, north-west). |
| **Perspective / Parallel** | Chooses the projection. **Shortcut:** `P`. |

Each control names its shortcut in its tooltip. The status bar at the bottom left tells you what is selected and whether snapping is on.

## Tips

- After a preset view, press `F` to frame the whole model in it.
- Screen readers announce the preset you chose and the projection, for example "Top view" or "Parallel projection".

## Troubleshooting

**The 3D view says "The 3D view isn't available on this computer".**
BIMCraft draws in 3D with your graphics card through a browser feature called WebGPU. Some computers don't offer it yet, notably Linux. Your project, its panels and editing still work; only the 3D view is missing. Update your operating system and graphics drivers; on Linux, a later version of BIMCraft will draw without WebGPU.

**The 3D view says "The 3D view stopped".**
The graphics card stopped responding, or the model was too big for it. Your model is safe. Choose **Reload view**. If it happens again within a short time, restart BIMCraft. **Copy details** copies a short technical description (no model content) for a bug report.

**The view stays on "Loading the 3D view…".**
After ten seconds it says "Still working…" and offers **Cancel**. Cancel, then choose **Reload view**.

**Letters such as `P` or `S` do nothing.**
These shortcuts are off while a dialog is open or while you type in a text field. Close the dialog or click the 3D view first.

## See also

- [Select and move elements](select-and-move-elements.md)
- [Place a block](place-a-block.md)
- [Use the workspace with the keyboard](use-the-workspace-with-the-keyboard.md)
