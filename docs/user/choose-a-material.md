# Choose a material

Every wall, slab, beam or column is made of a material, such as a concrete or a steel grade. BIMCraft ships a library of common European grades, each with the values a structural check needs, and lets you add your own.

## Before you start

- BIMCraft does not have a release yet. The material picker, its menu entries and command-palette names will be added to this page with the first version that shows it in the interface.

## The grades in the library

| Material | Grades | Values follow |
| --- | --- | --- |
| Concrete | C20/25, C25/30, C30/37, C35/45, C40/50, C45/55, C50/60 | EN 1992-1-1:2004 (Eurocode 2) |
| Reinforcing steel | B500B | EN 1992-1-1:2004, Annex C |
| Structural steel | S235, S275, S355, S420, S460 | EN 1993-1-1:2005 (Eurocode 3), for elements up to 40 mm thick |

A concrete grade such as C30/37 gives its characteristic cylinder strength, then its cube strength, in megapascals (MPa).

Every value comes with its **source**: the standard and the clause or table it is taken from, for example "EN 1992-1-1:2004, Table 3.1". Reports quote these sources, never the text of the standard. Check the values against your own copy of the standards and the national annex that applies to your project.

## What a material holds

- **Its category**: concrete, reinforcement, steel, timber, masonry, insulation, finish or generic.
- **Its behaviour law**, which says how stress follows strain:
  - *Linear elastic*: stress grows in proportion to strain, in tension and in compression.
  - *Parabola-rectangle* for concrete in compression: a curve up to the strain at peak stress, then a constant stress up to the ultimate strain. Concrete carries no tension.
  - *Bilinear* for steel: elastic up to the yield strength, then a slightly rising branch up to the ultimate strain (reinforcing steel), or a horizontal one (structural steel).
- **Its properties**: strengths, modulus of elasticity, Poisson's ratio, thermal expansion and density, each with its source.

The library values are *characteristic* values. Partial safety factors are not part of the material: they come from the design code and national annex of your project.

## Add your own grades

You can add grades that the library does not have, such as a timber grade or a product from a manufacturer's data sheet. A library file lists materials in the same form as the built-in ones. Each value is in SI units (pascals for strengths and moduli, kilograms per cubic metre for density, plain numbers for strains and ratios) and names its document and clause:

```json
{
  "format": 1,
  "materials": [
    {
      "name": "GL24h",
      "category": "other",
      "behaviour_law": "linear_elastic",
      "design_code": "EN 14080:2013",
      "properties": {
        "YoungModulus": { "value": 11.5e9, "document": "EN 14080:2013", "clause": "Table 5" }
      }
    }
  ]
}
```

**Result:** the new grades appear next to the built-in ones. A file is added completely or not at all: if one material in it is wrong, none is added.

## Tips

- Use the name the industry uses (C30/37, S355), so others recognise the grade.
- Give the edition of the standard in the document ("EN 1992-1-1:2004"): clause numbers change between editions.

## Troubleshooting

**"material #… has the name of another material"**
The file adds a grade that already exists, or lists one twice. Rename it, for example "C30/37 (project)".

**"material #… is not valid: the behaviour law needs the property …"**
The behaviour law reads a property that the material does not give. A linear elastic material needs its Young's modulus; a concrete needs its strength, peak and ultimate strains and parabola exponent; a steel needs its yield strength and Young's modulus.

**"material #… is not valid: the property … is out of range"**
A value is negative, zero or not physical, such as a Poisson's ratio of 0.5 or more. Check its unit: strengths are in pascals, so 30 MPa is written 30e6.

**"material #…: the source of … is not valid"**
A value has no document or no clause. Every value needs both.

**"the library file is not valid at line …, column …"**
The file is not valid JSON, or uses a key BIMCraft does not know. Check the place the message gives.

## See also

- [Save and open a project](save-and-open-a-project.md)
