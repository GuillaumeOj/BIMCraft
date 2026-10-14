# IFC 4.3 EXPRESS schema

`IFC4X3_ADD2.exp` is the official EXPRESS schema of IFC 4.3.2.0 (schema identifier `IFC4X3_ADD2`), the version adopted as ISO 16739-1:2024. `build.rs` generates the entity classes of `interop_ifc::ifc` from it ([ADR 0009](../../../docs/dev/adr/0009-ifc-entity-classes-from-express.md)).

| | |
| --- | --- |
| Source | <https://standards.buildingsmart.org/IFC/RELEASE/IFC4_3/HTML/IFC4X3_ADD2.exp> (Annex A, computer-interpretable listings) |
| Retrieved | 2026-10-14 |
| SHA-256 | `f67c8762b13a099c28082061e6f16b9ef1284ceec34069792afc702725675860` |
| Size | 403,825 bytes |
| Copyright | © buildingSMART International Ltd. |
| Licence | [Creative Commons Attribution-NoDerivatives 4.0 International (CC BY-ND 4.0)](https://creativecommons.org/licenses/by-nd/4.0/) |

**Never edit this file.** The licence allows redistribution only unmodified. To upgrade, replace it with a newer official release, and update the table above and the ADR.

To check the file:

```bash
shasum -a 256 crates/interop-ifc/schema/IFC4X3_ADD2.exp
```
