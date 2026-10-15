# Save and open a project

A BIMCraft project is a single file that holds your whole building model: every wall, slab, storey and their properties. You save your work into this file and open it again later, on the same computer or another one.

## Before you start

- BIMCraft does not have a release yet. The menu entries, shortcuts and command-palette names will be added to this page with the first version that can save from the interface.

## What a project file keeps

- **Every object and its values**, exactly as you entered them: lengths, angles and other quantities are stored in SI units (metres, radians…) and shown in your project's display units when you open it.
- **The version of the data model** that saved it. BIMCraft uses it to read files saved by earlier versions.

A project file is a single file. You can copy it, back it up or send it like any other document.

## Saving

Saving writes the complete model into the project file in one step. If the computer stops in the middle of a save, for example after a power cut, the file keeps the content of the previous save: it is never left half written.

## Opening

Opening reads the model from the file. Opening a project never changes the file; only saving does.

## Troubleshooting

**"The file is not a BIMCraft project"**
The file was made by another program, or is not a database at all. Check that you picked the right file. BIMCraft refuses to change files that are not its own.

**"The project uses schema version … this version reads up to …" or "The project uses version … of the "core" schema"**
The project was saved by a newer version of BIMCraft. Update BIMCraft to open it. The file is left unchanged.

**"The project file is damaged"**
Part of the file is missing or was changed by another program, for example when a copy was interrupted. Open a backup copy of the project.

**"The project does not fit the schema"**
The file contains objects with values BIMCraft does not accept, for example a wall without a required value. This usually means another program edited the file. Open a backup copy of the project.

## See also

- [Install BIMCraft](getting-started.md)
