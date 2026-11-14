import type { ViewState } from "../viewport/types";

/** A view state as the viewport reports it, with `changes` on top of an unremarkable one. */
export function viewState(changes: Partial<ViewState> = {}): ViewState {
  return {
    projection: "perspective",
    snapping: true,
    tool: "select",
    categories: [],
    disciplines: [],
    isolating: false,
    hiddenSelection: 0,
    sectionBox: null,
    clipPlanes: [],
    cutFaces: "tinted",
    ...changes,
  };
}

export const group = (name: string, flags: { hidden?: boolean; isolated?: boolean } = {}) => ({
  name,
  count: 3,
  hidden: flags.hidden ?? false,
  isolated: flags.isolated ?? false,
});
