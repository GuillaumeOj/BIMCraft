# wgpu renderer — UX brief

**No user-facing change in this feature.** Issue #68 is internal ("User docs: N/A"): it creates the `render` crate and proves, by an offscreen visual test, that models far from the world origin render without jitter. Nothing is shown in the application window yet.

- No design, UX or UI work, and no FigCraft board, are needed for #68.
- The viewport canvas, camera navigation (orbit, pan, zoom) and their keyboard and accessibility behaviour (UI-03, UI-06) belong to the later viewport issues, starting with #71 (interaction). The UX and UI designers will get a brief then.
