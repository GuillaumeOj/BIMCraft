# Set up the project location

Place your project on Earth: choose the map system your survey uses, say where your model's origin sits on that map, and how your building is turned. Do this once, near the start of a project, so that you can read and give map coordinates later.

## Before you start

- Have your survey or cadastral plan at hand. You need the name of its **coordinate system** (for example "Lambert 93" or "CC48") and the map coordinates of the point you want to use as the origin of your model.

A few words used on this page:

- **Coordinate system (CRS).** The map system that gives every place a pair of numbers, in metres. In France, Lambert 93 and the CC zones (CC42 to CC50) are common. Each one has a numeric code (for Lambert 93, EPSG 2154).
- **Base point.** The map coordinates of your model's origin, the point (0, 0, 0). Easting is the distance towards the east, northing the distance towards the north, and height the altitude.
- **Grid rotation.** The angle between the map's grid and your project's grid, counter-clockwise. Leave it at 0 if your building axes follow the map.
- **Grid north and true north.** Grid north is where the vertical lines of the map point. True north points to the geographic North Pole. They differ slightly, and the difference grows away from the centre of the map zone. BIMCraft computes the true north angle for you and draws project north, grid north and true north side by side.
- **Survey point.** A benchmark you know in map coordinates, such as a peg or a property corner. Once you enter it, BIMCraft shows where it sits in your model.

## Steps

1. Open the location dialog: choose **Project location…** in the title bar. **Shortcut:** Ctrl+Shift+L (Cmd+Shift+L on macOS).
2. In **1 Coordinate system**, type a code or part of a name, such as `2154` or `Lambert`. Choose one from the list with the arrow keys and Enter, or with the mouse. Its area of use appears under the field.
3. In **2 Project base point**, enter the Easting, Northing and Height of your model's origin, in metres. You can type a decimal point or a decimal comma.
4. In **3 Orientation**, enter the **Grid rotation**. Use **Decimal degrees** or **DMS** (degrees, minutes, seconds) as you prefer.
5. Optional: open **Advanced** to change the **Scale**. Leave it at 1 unless your surveyor gave you a project scale.
6. Optional: open **Survey point (optional)** and enter the Easting, Northing and Height of your benchmark. Fill in all three, or none.
7. Choose **Apply** (or press Enter).

**Result:** the dialog closes and a message confirms "Project location applied".

When you reopen the dialog, it shows what you entered together with the information BIMCraft derived from it: the true north angle, the latitude and longitude of the base point, the meridian convergence (the angle between grid north and true north), the point scale factor, and the survey point's position in the model.

## Options

| Option | What it does |
| --- | --- |
| Coordinate system | The map system of your survey. Only systems that give coordinates in metres are listed. |
| Easting, Northing, Height | Map coordinates of the model origin, in metres, to the millimetre. |
| Grid rotation | Angle between the map grid and the project grid, counter-clockwise, in degrees. |
| Scale | Project scale between 0.000001 and 1,000,000. Normally 1. |
| Survey point | A known benchmark in map coordinates, optional. |
| Remove georeference | Forgets the project's place on Earth. The model itself is not changed. Asks for confirmation, and can be undone. |

## Tips

- Use the **i** button next to a field for a short explanation of it.
- Press Esc to close the dialog without applying. Inside the coordinate-system list, Esc first closes the list.
- Choose **Apply** when nothing has changed to simply close the dialog.

## Troubleshooting

**"Choose a coordinate system."**
Pick a system from the list. Typing its name is not enough: choose the line you want.

**"This point is outside the area where this coordinate system is valid."**
The Easting and Northing do not fall inside the area of the chosen system. Check that you picked the right system (for example the right CC zone) and that you did not swap Easting and Northing.

**"The scale must be between 0.000001 and 1000000."**
Enter a scale close to 1, such as `1` or `0.9996`.

**"This project has several sites."**
BIMCraft can place a project that has exactly one site on Earth. Remove the extra sites first.

**I cannot find my coordinate system.**
Only the systems built into BIMCraft are listed, and only those that give coordinates in metres. Latitude and longitude systems cannot be used to place a project.

## See also

- [Read and enter coordinates](read-and-enter-coordinates.md)
- [Use the workspace with the keyboard](use-the-workspace-with-the-keyboard.md)
