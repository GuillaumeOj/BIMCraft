# BIMCraft

**An open-source BIM authoring tool for buildings and civil works, written from scratch in Rust.**

BIMCraft takes a construction project from early studies to detailed design in one shared model, across architecture, structure, MEP and thermal engineering. From that single model it produces 3D models, live 2D documentation (plans, sections, sheets) and traceable calculation reports.

> **Status: specification phase.** There is no code yet. The current work is the [specification](docs/specifications.md); reviews and comments from practitioners are the most useful contribution right now.

## Principles

- **One model, many views.** Every drawing, schedule and report is derived from the model, never drawn separately.
- **One public API.** The UI, Python plugins and the AI assistant (through MCP) all drive the same command and query API.
- **Platform-agnostic core.** The Rust core compiles natively and to WebAssembly: desktop first, browser later, same code.
- **IFC-aligned, not IFC-bound.** An internal model whose concepts map one-to-one onto IFC 4.3.
- **Progressive detail.** An element can start coarse and be refined to fabrication level without being redrawn.

## Planned scope

| Area | MVP | Later |
| --- | --- | --- |
| Platforms | Windows and macOS desktop | Linux, browser (WASM) |
| Disciplines | Structure (reinforced concrete, steel), architecture (building elements, curtain walls, massing, spaces, area tables) | Advanced architecture, timber, MEP, thermal, fire safety, civil works, bridges |
| Analysis | Linear static FEM, load takedown | Shells, Eurocode and other design-code checks |
| Interop | IFC 4.3 import and export | DXF, DWG, BCF, IDS, SAF, point clouds, glTF, Revit add-in |
| Extensibility | Command API, Python SDK | MCP server for AI assistants, plugin registry |

See the [roadmap](docs/specifications.md#15-roadmap-and-phases) for phases and their gates.

## Documentation

- [Specifications v0.1](docs/specifications.md): vision, architecture, requirements (with stable IDs such as `CORE-01`), roadmap and open questions.
- [User guide](docs/user/introduction.md): how to use BIMCraft, by task.
- [AGENTS.md](AGENTS.md): rules for contributors and coding agents; [quality rationale](docs/dev/quality.md).

## Contributing

Contributions are welcome, from code to domain expertise in any engineering field. Read [CONTRIBUTING.md](CONTRIBUTING.md) first: every commit must be signed off under the Developer Certificate of Origin.

## Licence

BIMCraft is released under the [MIT licence](LICENSE).

## Disclaimer

BIMCraft is provided without warranty. Calculation results must always be checked by a qualified engineer before being used for design or construction.
