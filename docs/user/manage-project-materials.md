# Manage the project's materials

Every wall, slab, beam or column is made of a material, such as a concrete or a steel grade. In the **Materials** dialog you add materials to your project from a library of common European grades, create your own, check or change their values, and remove the ones you no longer need.

## Before you start

- Nothing: every project starts with no material. You add the ones you need.

A few words used on this page:

- **Grade.** A standard class of material, such as concrete C25/30 or steel S355. The library holds grades; your project holds *materials*, which are copies of grades (or ones you created).
- **Behaviour law.** How stress follows strain in the material, for example *linear elastic* or *parabola-rectangle* (concrete in compression). It comes with the grade and is shown, not edited.
- **Source.** The standard and the clause or table a value comes from, such as "EN 1992-1-1:2004 · Table 3.1". Reports quote these sources, never the text of the standard.

## Open the Materials dialog

Choose **Materials…** in the title bar. **Shortcut:** Ctrl+Shift+M (Cmd+Shift+M on macOS). The shortcut does nothing while another dialog is open.

The list of the project's materials is on the left and the selected material on the right. Press **Done** or Esc to close the dialog; every change is already saved.

## Add a grade from the library

1. In the Materials dialog, choose **Add from library…**. A project with no material opens the library directly.
2. Type in **Search the library** (a name such as `C30`), or choose a category under the search field. Use the up and down arrows to move through the grades.
3. Select a grade to see its values and sources on the right. A grade that is already in your project shows "In the project".
4. Optional: change **Name in this project**. The values are copied into your project; later updates of the library never change your materials. Two materials cannot have the same name (capital letters do not count).
5. Choose **Add to project** (or press Enter in the name field).

**Result:** the material is added and selected, and a message such as "C25/30 added" appears with an **Undo** button.

## Create your own material

1. Choose **New material…**.
2. Enter a **Name** (1 to 120 characters) and choose a **Category**.
3. Choose **Create**.

**Result:** the material opens with every value empty. Use **Add value** on a row to enter what you know. Values you enter are marked **Entered by user**.

## Read and change a value

1. Select a material in the list. Use the **Mechanical** and **Thermal** tabs to move between its properties. The Graphics and Environmental tabs are shown so that you know what is coming; they are not available yet.
2. Each row shows the property, its value and its source. A property is always shown in the same unit: megapascals (MPa) for strengths, gigapascals (GPa) for moduli, kg/m³ for density, W/(m·K) for thermal conductivity and ‰ for strains. A partial factor has one line for each national annex (Recommended and France).
3. To change a value, choose it (or press Enter or F2 on it). Type the new number: a decimal point or a decimal comma both work. Press Enter or leave the field to save it; press Esc to cancel.

**Result:** the row shows the badge **Entered by user** with a coloured rule, and the change is one step you can undo.

If a value is not accepted, a message under the field explains why and, when it is known, the allowed range, for example "Between 0 and 0.5". The field stays open so you can correct it.

## Go back to the library value

On a value you changed, choose **Reset to library value**. Point at the button to see the library value first. The badge disappears and the source comes back.

A material you created has no library value: clear the field to remove a value. A required value, such as the strength of a concrete, cannot be cleared.

## Rename or delete a material

1. Select the material, then open the **…** menu next to its name and choose **Rename…** or **Delete…**. From the list, F2 renames and Delete deletes.
2. To rename, type the new name and press Enter (Esc cancels).
3. To delete, confirm in the bar at the bottom: **Keep material** is selected by default; choose **Delete material** to remove the material. A message appears with an **Undo** button.

A material that build-up layers use cannot be deleted. The dialog tells you how many layers use it: change their material first, then delete it.

## Options

| Option | What it does |
| --- | --- |
| Search materials | Shows the materials whose name contains the text. |
| Category | Shows one category of materials only. |
| Add from library… | Adds a grade from the library to the project. |
| New material… | Creates a material of your own. |
| Reset to library value | Puts a changed value back to the grade's value and source. |

## The grades in the library

| Material | Grades | Values follow |
| --- | --- | --- |
| Concrete | C12/15, C16/20, C20/25, C25/30, C30/37, C35/45, C40/50, C45/55, C50/60 | EN 1992-1-1:2004 (Eurocode 2) |
| Reinforcing steel | B500A, B500B, B500C | EN 1992-1-1:2004, Annex C |
| Structural steel | S235, S275, S355, S420, S460 | EN 1993-1-1:2005 (Eurocode 3), for elements up to 40 mm thick |
| Timber | C16 to C35, GL24h, GL28h, GL32h | EN 338:2016 and EN 14080:2013 |
| Masonry | Three common clay and concrete-block masonries | EN 1996-1-1:2005 |
| Other | Insulation, finishes and generic materials | Règles Th-Bât, Fascicule Matériaux (2017) |

A concrete grade such as C30/37 gives its characteristic cylinder strength, then its cube strength, in megapascals (MPa). The library values are *characteristic* values. Partial safety factors come with the grade for each national annex, so check them, like every value, against your own copy of the standards and the national annex that applies to your project.

## Add grades from a library file

To add many grades at once, or to share them with a team, you can add grades that the library does not have, such as a product from a manufacturer's data sheet, from a library file. A library file lists materials in the same form as the built-in ones. Each value is in SI units (pascals for strengths and moduli, kilograms per cubic metre for density, plain numbers for strains and ratios) and names its document and clause:

```json
{
  "format": 1,
  "materials": [
    {
      "name": "Supplier board",
      "category": "generic",
      "behaviour_law": "linear_elastic",
      "properties": {
        "e_modulus": { "value": 4.0e9, "document": "Supplier data sheet 2025", "clause": "Table 2" }
      }
    }
  ]
}
```

**Result:** the new grades appear next to the built-in ones in the library. A file is added completely or not at all: if one material in it is wrong, none is added.

## Tips

- Use the name the industry uses (C30/37, S355), so others recognise the grade.
- Give the edition of the standard in the document ("EN 1992-1-1:2004"): clause numbers change between editions.
- Undo (Ctrl+Z, Cmd+Z on macOS) takes back an edit, a rename, an addition or a deletion.

## Troubleshooting

**"A material with this name already exists."**
Names must be different in a project, whatever the capital letters. Choose another name, for example "C30/37 (slab)".

**"Enter a valid value for this property."**
The text is not a number, or the value is outside what the material accepts. The message shows the allowed range when there is one. Enter the value in the unit shown in the field.

**"This value is required."**
A strength such as the one of a concrete must have a value. Type a number instead of clearing the field.

**Delete… says that build-up layers use the material.**
Open the build-ups that use it, choose another material for those layers, then delete this one.

**The Graphics and Environmental tabs do nothing.**
They are not available yet.

**"material #… has the key of another material"**
The file adds a grade that already exists, or lists one twice. Rename it, for example "C30/37 (project)".

**"material #… is not valid: the material needs the property …"**
The behaviour law, or the category, needs a property that the material does not give. A linear elastic material needs its Young's modulus; a concrete needs its strength, peak and ultimate strains and parabola exponent; a steel needs its yield strength and Young's modulus.

**"material #… is not valid: the property … is out of range"**
A value is negative, zero or not physical, such as a Poisson's ratio of 0.5 or more. Check its unit: strengths are in pascals, so 30 MPa is written 30e6.

**"material #…: the source of … is not valid"**
A value has no document or no clause. Every value needs both.

**"the library source is not valid at line …, column …"**
The file is not valid JSON, or uses a key BIMCraft does not know. Check the place the message gives.

## See also

- [Save and open a project](save-and-open-a-project.md)
