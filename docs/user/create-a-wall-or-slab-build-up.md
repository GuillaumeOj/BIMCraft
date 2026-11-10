# Create a wall or slab build-up

A build-up lists the layers of a wall or a slab, from one face to the other: for example a finish, an insulation layer, a concrete core and a plaster board. Build the composition once, with its materials and thicknesses, and reuse it.

## Before you start

- The materials the layers are made of must be in your project. You can add them while you build the build-up (see [Manage the project's materials](manage-project-materials.md)).

A few words used on this page:

- **Layer.** One band of material of a given thickness. A build-up has between 1 and 32 layers.
- **Function.** What the layer is for: structure, substrate, insulation, air gap, membrane or finish.
- **Core.** The load-bearing part of the build-up. The core layers must be next to each other.
- **Join priority.** When two walls meet, the layers with a higher priority (1) run through the layers with a lower priority (5); layers with the same priority meet. Each function has a default: structure 1, substrate 2, insulation 3, air gap 3, membrane 4, finish 5.
- **Thermal resistance (R).** How well a layer, or the whole build-up, resists the flow of heat, in m²·K/W. BIMCraft calculates it from the thickness and the thermal conductivity of each material.

## Steps

1. Choose **Build-ups…** in the title bar. **Shortcut:** Ctrl+Shift+B (Cmd+Shift+B on macOS). The shortcut does nothing while another dialog is open.
2. Choose **New build-up…** and then **Wall build-up** or **Slab build-up**. (In an empty project, choose **New wall build-up** or **New slab build-up**.) A wall build-up lists its layers from the exterior to the interior; a slab build-up from the top to the bottom. You cannot change this afterwards.
3. Enter a **Name**, and a **Description** if you like.
4. Fill in the first layer: choose its **Function** and its **Material**, and enter its **Thickness** in millimetres. A decimal point and a decimal comma both work. Only a membrane may be 0 mm thick.
5. Choose **Add layer** for each further layer (or press Ctrl+Enter in a layer: the new layer goes after it).
6. Optional: adjust the **Join priority** or the **Core** box of a layer. A priority you changed is marked **Modified**; choose the option marked "(default)" in the list to go back.
7. Choose **Apply**. **Shortcut:** Enter in a text field.

**Result:** the build-up appears in the list, with a message "Build-up applied" and an **Undo** button. Applying is one undo step.

## Check the section and the totals

While you edit, the dialog shows a **Section preview** (a schematic, not to scale) and the **Total thickness** and **Thermal resistance**. BIMCraft asks its calculation core for these figures, so they are always the real ones.

If a layer has a material without a thermal conductivity, such as an air gap, its resistance shows "—". The total counts the other layers and a note says how many layers were left out. Surface resistances and the U-value are not part of this total.

## Reorder, add and remove layers

- **Move layer … up** and **Move layer … down** move a layer one place. With the keyboard, press **Alt+↑** or **Alt+↓** on one of the layer's buttons or on its Core box. Focus stays on the control you used, and the new position is read out by screen readers.
- **Remove layer …** removes a layer. A build-up always keeps at least one.
- A build-up can have at most 32 layers.

## Choose or add a material

In the **Material** list, choose a material of your project. The **Manage materials…** button next to the list opens the Materials dialog on top of the editor: add or create a material, press **Done**, and the new material is chosen for the layer. An **air gap** layer can only use generic materials.

## Edit, duplicate, rename and delete a build-up

In the list, open the **…** menu of a build-up:

| Option | What it does |
| --- | --- |
| Edit… | Opens the editor. Enter or a double click on the row does the same. |
| Duplicate… | Makes a copy under a name you choose. |
| Rename… | Changes the name in the row. F2 does the same. |
| Delete… | Deletes the build-up and its layers after a confirmation. Delete does the same. You can undo it. |

Use **Walls**, **Slabs** or **All** to filter the list.

## Cancel your changes

Choose **Cancel** or press Esc. If you changed something, BIMCraft asks "Discard your changes?": **Keep editing** is selected by default.

## Troubleshooting

**"Choose a material." under a layer.**
Every layer needs a material. Choose one in the list.

**"Enter a thickness greater than 0 mm."**
A layer needs a thickness. Only a membrane may be 0 mm.

**"Core layers must be next to each other."**
The layers marked Core have a layer in between that is not part of the core. Tick or untick the Core boxes so that the core is one block.

**"A build-up with this name already exists."**
Choose another name; capital letters do not count.

**"This material cannot be used for an air gap."**
An air gap takes a generic material. Choose one, or change the function of the layer.

**"This build-up changed since you opened it."**
Another change reached the build-up while you were editing. Close it and open it again.

## See also

- [Manage the project's materials](manage-project-materials.md)
- [Use the workspace with the keyboard](use-the-workspace-with-the-keyboard.md)
