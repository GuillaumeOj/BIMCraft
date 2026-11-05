# Read and enter coordinates

Type a point in any one of three coordinate systems and read it in the other two: model coordinates, map coordinates, and latitude and longitude. Use it to check where something is on the map, or to find a surveyor's point in your model.

## Before you start

- The project must have a location. If it has none, the panel says so and offers **Set project location…**. See [Set up the project location](set-up-the-project-location.md).

## Steps

1. Show the panel: choose the **Coordinates** tab at the right of the window. **Shortcut:** Ctrl+Shift+K (Cmd+Shift+K on macOS).
2. The panel opens on your model's origin, shown in three blocks: **Model (local)** (x, y, z in your project's axes), **Map** (Easting, Northing and Height in the project's coordinate system) and **Geographic** (latitude, longitude and height on the system's datum).
3. Type a value in any field of any block. The block you type in is marked **Edited**, and the other two show **Computed from** that block a moment later. Press Enter to convert straight away.
4. To copy a block, choose **Copy** in its title. The three values are copied separated by tabs, so they paste into spreadsheet cells.

**Result:** the three blocks describe the same point. Lengths are shown in metres to the millimetre.

## Options

| Option | What it does |
| --- | --- |
| Decimal degrees / DMS | Shows latitude and longitude as decimal degrees (eight decimals, about 1 mm) or as degrees, minutes and seconds. You can type either form. |
| Change… | Opens the project location dialog. |
| Reset to origin | Goes back to the model origin (0, 0, 0). |
| Pick from view | Reserved for choosing a point with the mouse in a view. Not available yet. |

## Tips

- You can type a decimal point or a decimal comma. In DMS, write for example `48°51′10.8″N`, or `S 10 15 30`.
- The height in the Geographic block is approximate: BIMCraft does not model the geoid (the shape of mean sea level) yet.

## Troubleshooting

**"Enter a number, e.g. 652000.000."**
A field holds text that is not a number. Correct it and press Enter.

**"This point is too far from the projection to convert."**
The point lies outside the area where the coordinate system works. Check the numbers, or the coordinate system of the project.

**The panel shows "This project has no location yet."**
Choose **Set project location…** and follow [Set up the project location](set-up-the-project-location.md).

## See also

- [Set up the project location](set-up-the-project-location.md)
