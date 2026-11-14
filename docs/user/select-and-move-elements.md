# Select and move elements

Select elements in the 3D view to see them highlighted, then move them, either by dragging or precisely with the Move tool and a typed distance. Moves snap to corners, edge midpoints, edges, faces and the ground, and each move can be undone.

## Before you start

- The model has at least one element, for example a [block](place-a-block.md).

A few words used on this page:

- **Snapping.** While you point, BIMCraft catches the nearest remarkable point within a few pixels: an endpoint (corner), the midpoint of an edge, a point on an edge or a face, or the ground. A marker and a label show which one.
- **Base point / target point.** The point you move from and the point you move to.

## Select elements

1. With the **Select** tool (**Shortcut:** `V`), move the pointer over the model: the element under it gets an amber outline.
2. Click an element to select it. It gets a teal outline and tint.
3. To add elements, Shift-click them. To add or remove one, Ctrl-click it (Cmd-click on macOS).
4. To clear the selection, click an empty part of the view or press **Esc** in the view.

**Result:** the status bar at the bottom left reads, for example, "2 elements selected · Esc to clear", and screen readers announce the count.

There is no way yet to pick an element with the keyboard alone; keyboard users will select from the project tree when it arrives.

## Move by dragging

1. Drag a selected element (an unselected one is selected first). A ghost stays at the starting place while the element follows the pointer, snapping to the points of other elements.
2. Release the button to drop it.

**Result:** the element moves, and a message such as "Moved 1 element by 4.000 m" appears with an **Undo** button.

## Move precisely with the Move tool

1. Select the elements to move.
2. Choose **Move** in the tool bar at the bottom. **Shortcut:** `M`. Without a selection, BIMCraft says "Select something to move".
3. Click the base point; snapping helps you pick a corner or a midpoint.
4. Point in the direction of the move. A rubber-band line and a chip show the distance.
5. Either click the target point, or type the distance in metres (for example `2.5`, with a point as the decimal separator) and press **Enter**.

**Result:** the elements move, and a message such as "Moved 1 element by 2.500 m" appears with an **Undo** button.

Press **Esc** to cancel a move before it ends.

## Options

| Option | What it does |
| --- | --- |
| Hold **Alt** | Suspends snapping while held. |
| `S` | Turns snapping on or off. The status bar shows "Snap on" or "Snap off". |
| **Undo** in the message | Puts the elements back where they were. |

## Troubleshooting

**"Can't move this element yet".**
This kind of element cannot be moved by this tool yet. Only blocks can move for now.

**"Too many elements to move at once".**
A single move takes at most 10,000 elements. Select fewer and move them in several steps.

**The move snaps to the wrong point.**
Zoom in so points are further apart on screen, or hold **Alt** to place it freely.

## See also

- [Navigate the 3D view](navigate-the-3d-view.md)
- [Place a block](place-a-block.md)
