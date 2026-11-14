# Place a block

A block is a simple box: use it for massing studies, placeholders or anything you will model in detail later. You place it with two clicks for its footprint and a height, or by typing its sizes.

## Before you start

- Nothing: blocks can be placed in an empty project.

A few words used on this page:

- **Footprint.** The rectangle the block covers on the ground: its length and its depth.
- **Snapping.** While you point, BIMCraft catches the nearest corner, edge midpoint, edge, face or the ground, and shows a marker and its name.

## Steps

1. Choose **Block** in the tool bar at the bottom of the window. **Shortcut:** `B`. In an empty project, the **Place a block** button in the middle of the view does the same.
2. Click the first corner of the footprint, on the ground or on another element.
3. Click the second corner, or type the length in metres and press **Enter**, then the depth and **Enter**. A dashed rectangle with its live size (for example "6.00 × 4.00 m") follows the pointer.
4. Move the pointer up to set the height, or type it and press **Enter**. Pressing **Enter** without a value gives 3.00 m.

**Result:** the block appears, selected, and a message such as "Block placed · 6.00 × 4.00 × 3.00 m" appears with an **Undo** button. The Block tool stays active so you can place the next one; press **Esc** or `V` to go back to Select.

## Options

| Option | What it does |
| --- | --- |
| Typed values | Sizes in metres, with a point as the decimal separator. Each value ends with **Enter**, never Tab. |
| **Esc** | Goes back one step: from the height to the footprint, then to the first corner, then out of the tool. |
| Hold **Alt** | Suspends snapping while held. |
| `S` | Turns snapping on or off. |

## Tips

- To line blocks up, start the footprint on a corner of an existing block: the endpoint snap catches it.
- Hints above the tool bar say what the current step expects.

## Troubleshooting

**Typed numbers do nothing.**
The 3D view must have the keyboard: click it, or press Tab until it shows a green outline, then type.

**The block tool is greyed out.**
Tools are off while the 3D view loads, when it is not available on this computer, or after it stopped. See [Navigate the 3D view](navigate-the-3d-view.md#troubleshooting).

## See also

- [Select and move elements](select-and-move-elements.md)
- [Navigate the 3D view](navigate-the-3d-view.md)
