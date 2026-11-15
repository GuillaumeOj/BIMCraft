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

## App icon

The app icon is an isometric building seen like an X-ray: the finished facade at the bottom fades into the bare concrete frame, which fades into the rebar cage and its starter bars at the top.

- **Masters:** [`crates/desktop-app/icons/source/logo.svg`](../../crates/desktop-app/icons/source/logo.svg) is the full drawing, used from 48 px up. [`logo-small.svg`](../../crates/desktop-app/icons/source/logo-small.svg) is the same building simplified for 16, 24 and 32 px (window bands, thick columns, no stirrups or mesh), because the details turn to noise there. Edit both together. The web favicon (`ui/public/favicon.svg`) is a copy of the small master.
- **Layers:** each master marks its layers with `<!-- layer:<name> -->` … `<!-- /layer -->` comments: `background`, `rebar`, `frame` and `facade`, from back to front. Keep shapes flat (no shadows or highlights) and keep the background a single `#RRGGBB` fill: macOS adds the glass, light and depth itself.
- **Generated files:** after changing a master, run `bun run icons` in `ui/` (or `bun scripts/build-icons.ts`) and commit the results. It uses the pinned Tauri CLI to write, in `crates/desktop-app/icons/`:
  - `32x32.png` (from the small master), `128x128.png`, `128x128@2x.png` and `icon.png` for Linux and the window icon;
  - `icon.ico` for Windows, with 16, 24 and 32 px images from the small master and 48, 64 and 256 px from the full one;
  - `icon.icns`, the macOS icon before macOS 26, shrunk onto Apple's 824 px icon grid;
  - `BIMCraft.icon`, an Icon Composer file for macOS 26 and later: the master's background colour as the fill, and one glass group per layer (`facade`, `frame`, `rebar`, as 1024 px PNG images). When bundling on macOS, Tauri compiles it with `actool` (Xcode 26 or later) into `Assets.car`, which gives the Liquid Glass icon in its default, dark, clear and tinted appearances.
- **Preview:** open `BIMCraft.icon` in Icon Composer (Xcode 26 or later) to check the glass appearances, and look at the 16 and 32 px PNG files at full zoom.
