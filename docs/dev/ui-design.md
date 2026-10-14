# UI design

The BIMCraft interface is designed in [Penpot](https://penpot.app), an open-source design tool. The Penpot file is the source of truth for screen layout, components, colours, typography and icons. It is not committed to the repository.

**Design file:** [BIMCraft UI in Penpot](https://design.penpot.app/#/view?file-id=0f3fb038-01f9-8046-8008-c9e2e899edf0&page-id=0f3fb038-01f9-8046-8008-c9e2e899edf1&share-id=2730878a-57d5-4f88-b507-730e8279e188) (view-only share link)

## Reading the design

- **In a browser:** open the link above. Viewing needs no account; editing needs an invitation to the Penpot team.
- **From a coding agent:** connect the [Penpot MCP server](https://github.com/penpot/penpot-mcp) in your agent, then open the file in the Penpot editor and start the MCP plugin there. Plugins run only in the editor, so this needs edit access to the file; the view-only link is not enough. Start with its `high_level_overview` tool, then inspect or export the shapes you need. The repository commits no MCP configuration; each contributor connects their own.

## Rules

- Read the relevant board in the design before building or changing a screen or component, and implement from it.
- When the code has to differ from the design — accessibility (`UI-06`), longer French strings (`UI-04`), command-palette or shortcut reachability (`UI-03`) — say so in the pull request so the design is updated. Do not let code and design drift silently.
- Text shown in the design is a placeholder for layout: externalise every string (`UI-04`), never hardcode it from the mockup.
