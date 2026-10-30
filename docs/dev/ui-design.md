# UI design

The BIMCraft interface is designed in [FigCraft](https://github.com/GuillaumeOj/figcraft), an open-source interface design tool. The file [`docs/BIMCraft.figcraft`](../BIMCraft.figcraft) is the source of truth for screen layout, components, colours, typography and icons. The design was moved there from Penpot, which is no longer used for it.

The file is stored with Git LFS (`*.figcraft` in `.gitattributes`). Run `git lfs install` once, then `git lfs pull` if the file shows as a short text pointer.

## Structure of the file

- **Pages:** `Icons`, `Brand guide`, `Screens`, then one page per flow: `1 · Start and projects`, `2 · Site and georeferencing`, `3 · Architecture modelling`, `4 · Structure modelling`, `5 · Analysis and reports`, `6 · Documentation`, `7 · Interoperability`, `8 · Extensibility and system`. Each screen exists in a light and a dark board (`… · light`, `… · dark`).
- **Components:** the icon masters are on `Icons`, and the UI components (buttons, tags, search field, tool buttons, property row) are in the cards of `Brand guide`. Icons in the screens are instances of them.
- **Variables:** the `Mode` collection holds the colour tokens with a Light and a Dark mode (`color/bg`, `color/surface`, `color/acc`…), and `Core` holds radii, spacing and sizes. Dark boards use the Dark mode.

## Reading the design

- **In FigCraft:** open `docs/BIMCraft.figcraft`. Pages and layers are in the left panel; Inspect shows sizes, colours and variable names.
- **From a coding agent:** connect the FigCraft MCP server (`figcraft` for a headless session, `figcraft-app` to drive the running app) and open the file. Use `inspect_document` or `get_node` for the layer tree, `screenshot` to look at a board, and the `inspect.code` command for generated CSS or React. The repository commits no MCP configuration; each contributor connects their own.

## Rules

- Read the relevant board in the design before building or changing a screen or component, and implement from it.
- When the code has to differ from the design — accessibility (`UI-06`), longer French strings (`UI-04`), command-palette or shortcut reachability (`UI-03`) — note the difference in the pull request so the design can be updated.
- Text in the design is a layout placeholder; strings still go through the translation files (`UI-04`).
- Change the design in the FigCraft file and commit it with the code that needs it. Because it is a binary file, avoid two branches editing it at once.
