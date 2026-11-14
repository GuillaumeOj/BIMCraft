# Hide and isolate elements

Hide the elements you do not need, or isolate the ones you want to look at, by category, by discipline or by selection. Hiding and isolating only change what you see in the 3D view; they never change the model, and they are forgotten when you close the window.

## Before you start

- The model has at least one element, for example a [block](place-a-block.md).

A few words used on this page:

- **Category.** The kind of element: walls, slabs, columns…
- **Discipline.** The trade an element belongs to: architecture, structure…
- **Isolate.** Show only some elements and hide all the others.

## Hide or show a category or a discipline

1. In the view controls at the bottom right, choose **Visibility** (the eye). The Visibility list opens, with the first line selected.
2. Pick **Category** or **Discipline** at the top.
3. Click a line to hide it, and click it again to show it. With the keyboard, move with **↑**, **↓**, **Home** and **End**, and press **Space** to hide or show.

**Result:** the line turns grey and reads "Hidden", and its elements disappear from the view. Hidden elements cannot be picked or snapped to. The View panel lists them under **Visibility**.

Press **Esc** to close the list; the focus returns to the eye button. You can also click anywhere outside it.

## Isolate

**A category or a discipline.** In the Visibility list, press **Shift+Space** on a line, or Shift-click it. The line is tagged "Isolated" and everything else is hidden. Do it on a second line to show both, and on an isolated line again to take it out.

**The selection.** Select elements, then choose **Isolate selection** in the list, or press `I`.

**Result:** a message reads, for example, "Isolate · only Structure is shown · Esc or “End isolate” to return". The View panel is tagged "Isolated".

## Hide the selection

Select elements, then choose **Hide** in the list, or press `H`. The elements are hidden, and hidden elements cannot be selected.

## Bring everything back

| What | How |
| --- | --- |
| End an isolate | **End isolate** in the View panel, or **Esc** (it works wherever the focus is, except in a text field). |
| Show everything | **Reset visibility** in the View panel, or `Alt+H`. This clears hidden categories, hidden disciplines, hidden selections and any isolate. |

## Keyboard shortcuts

| Shortcut | What it does |
| --- | --- |
| `H` | Hide the selection. |
| `I` | Isolate the selection. |
| `Alt+H` | Reset visibility. |
| **Esc** | End an isolate. |

`H` and `I` are single-key shortcuts. You can turn those off with **Single-key shortcuts** in the title bar; they then work only while the 3D view has the focus. `Alt+H` is not affected.

## Troubleshooting

**"Select something to hide or isolate".**
`H`, `I` and the two buttons act on the selection, and nothing is selected. Select an element first.

**An element is missing from the view.**
It may be hidden, outside the section box or behind a clipping plane. Look at the **Visibility** section of the View panel, then at [Cut the model with a section box](cut-the-model-with-a-section-box.md).

**The Visibility list is empty.**
The model has no elements of that kind yet. It lists what the model contains.

**Pressing `H` or `I` types a letter.**
The focus is in a text field, where letters are text. Press **Esc**, or click in the 3D view, then try again.

## See also

- [Cut the model with a section box](cut-the-model-with-a-section-box.md)
- [Select and move elements](select-and-move-elements.md)
