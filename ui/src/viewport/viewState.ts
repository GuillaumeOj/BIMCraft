import type { Vector3, ViewState, VisibilityGroup } from "./types";

/** At most this many user clipping planes (api-contract.md §2, B8). */
export const MAX_CLIP_PLANES = 6;

export type SectionBox = NonNullable<ViewState["sectionBox"]>;
export type ClipPlane = ViewState["clipPlanes"][number];
export type Axis = 0 | 1 | 2;
export type Side = "min" | "max";

export const AXES: readonly Axis[] = [0, 1, 2];
export const AXIS_NAMES = ["X", "Y", "Z"] as const;

/** The indices (0–5) of the faces whose position differs between two boxes: min x, y, z, then max x, y, z. */
export function changedFaces(before: SectionBox, after: SectionBox): number[] {
  const a = [...before.min, ...before.max];
  const b = [...after.min, ...after.max];
  return a.flatMap((value, index) => (value === b[index] ? [] : [index]));
}

/** A box with `value` at `side` of `axis`. */
export function withExtent(box: SectionBox, axis: Axis, side: Side, value: number): SectionBox {
  const edit = (vector: Vector3): Vector3 => {
    const next: Vector3 = [...vector];
    next[axis] = value;
    return next;
  };
  return side === "min" ? { ...box, min: edit(box.min) } : { ...box, max: edit(box.max) };
}

/** Whether `value` can replace the `side` extent of `axis`: finite, and the minimum stays below the maximum. */
export function extentIsValid(box: SectionBox, axis: Axis, side: Side, value: number): boolean {
  if (!Number.isFinite(value)) return false;
  return side === "min" ? value < box.max[axis] : value > box.min[axis];
}

const EPSILON = 1e-6;

/** The axis a plane's normal runs along and its sign, or null for an oblique plane. */
export function axisNormal(normal: Vector3): { axis: Axis; sign: "+" | "−" } | null {
  for (const axis of AXES) {
    const along = Math.abs(Math.abs(normal[axis]) - 1) < EPSILON;
    const square = AXES.every((other) => other === axis || Math.abs(normal[other]) < EPSILON);
    if (along && square) return { axis, sign: normal[axis] > 0 ? "+" : "−" };
  }
  return null;
}

/** The names of the groups that satisfy `pick`, in the order the viewport reports them. */
export const groupNames = (
  groups: readonly VisibilityGroup[],
  pick: (group: VisibilityGroup) => boolean,
): string[] => groups.filter(pick).map((group) => group.name);

/** The names of the hidden and of the isolated groups. */
export const hiddenNames = (groups: readonly VisibilityGroup[]) =>
  groupNames(groups, (group) => group.hidden);
export const isolatedNames = (groups: readonly VisibilityGroup[]) =>
  groupNames(groups, (group) => group.isolated);
