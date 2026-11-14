// The TypeScript side of the viewport module's interface (api-contract.md §2). The generated
// bindings type every argument as `any`; these types are what the Rust side validates.

export type Vector3 = [number, number, number];

export type ViewportTheme = {
  background: string;
  selection: string;
  hover: string;
  snap: string;
  preview: string;
  cut: string;
};

export type ViewportOptions = { theme: ViewportTheme };

export type Tool = "select" | "move" | "place_block";

export type Preset =
  | "top"
  | "front"
  | "back"
  | "left"
  | "right"
  | "iso_se"
  | "iso_sw"
  | "iso_ne"
  | "iso_nw";

export type Projection = "perspective" | "parallel";

export type VisibilityTarget =
  | { by: "category"; names: string[] }
  | { by: "discipline"; names: string[] }
  | { by: "selection" };

export type ViewCommand =
  | { kind: "zoom_fit" }
  | { kind: "zoom_selection" }
  | { kind: "view_preset"; preset: Preset }
  | { kind: "projection"; projection: Projection }
  | { kind: "select"; ids: string[] }
  | { kind: "clear_selection" }
  | { kind: "cancel" }
  | { kind: "typed_distance"; metres: number }
  | { kind: "snapping"; enabled: boolean }
  | { kind: "section_box"; enabled: boolean }
  | { kind: "section_box_extents"; min: Vector3; max: Vector3 }
  | { kind: "section_box_fit_selection" }
  | { kind: "section_box_reset" }
  | { kind: "clip_plane_pick" }
  | { kind: "clip_plane_add"; point: Vector3; normal: Vector3 }
  | { kind: "clip_plane_flip"; index: number }
  | { kind: "clip_plane_remove"; index: number }
  | { kind: "clip_planes_clear" }
  | { kind: "cut_faces"; style: "tinted" | "none" }
  | { kind: "hide"; target: VisibilityTarget }
  | { kind: "show"; target: VisibilityTarget }
  | { kind: "isolate"; target: VisibilityTarget }
  | { kind: "end_isolate" }
  | { kind: "reset_visibility" };

export type ToolStep =
  | "idle"
  | "base_point"
  | "target_point"
  | "first_corner"
  | "second_corner"
  | "height"
  | "pick_face";

export type AnnouncementKey =
  | "snap.endpoint"
  | "snap.midpoint"
  | "snap.edge"
  | "snap.face"
  | "snap.ground"
  | "selection.count"
  | "selection.cleared"
  | "move.distance"
  | "move.cancelled"
  | "place.size"
  | "place.height"
  | "view.preset"
  | "view.projection"
  | "visibility.hidden"
  | "visibility.isolated"
  | "visibility.reset"
  | "clip_plane.added"
  | "clip_plane.flipped"
  | "clip_plane.removed"
  | "clip_plane.cleared"
  | "section_box.face_moved";

export type Intent =
  | { kind: "move"; ids: string[]; translation: Vector3 }
  | { kind: "place_block"; location: Vector3; rotation: number; size: Vector3 };

export type VisibilityGroup = { name: string; count: number; hidden: boolean; isolated: boolean };

export type ViewState = {
  projection: Projection;
  snapping: boolean;
  tool: Tool;
  categories: VisibilityGroup[];
  disciplines: VisibilityGroup[];
  isolating: boolean;
  hiddenSelection: number;
  sectionBox: { enabled: boolean; min: Vector3; max: Vector3 } | null;
  clipPlanes: { point: Vector3; normal: Vector3 }[];
  cutFaces: "tinted" | "none";
};

export type PreviewLabel = {
  /** CSS pixels from the canvas's top left. */
  at: [number, number];
  key: "move.distance" | "place.size" | "place.height";
  /** Metres. */
  values: Record<string, number>;
  /** The raw typed buffer, if any. */
  typed: string | null;
};

export type ViewportErrorKey = "device_lost" | "scene_rejected" | "out_of_memory";

export type ViewportEvent =
  | { kind: "selection"; ids: string[] }
  | { kind: "hover"; id: string | null }
  | { kind: "tool_state"; tool: Tool; step: ToolStep }
  | { kind: "announce"; key: AnnouncementKey; values: Record<string, string | number> }
  | { kind: "intent"; intent: Intent }
  | { kind: "view_state"; state: ViewState }
  | { kind: "preview_label"; label: PreviewLabel | null }
  | { kind: "error"; key: ViewportErrorKey };

/** One viewport on a canvas (the module's `Viewport` class, typed). */
export interface ViewportHandle {
  free(): void;
  applyScene(bytes: Uint8Array): number;
  /** The revision held, or null before the first delta. */
  revision(): number | null;
  resize(width: number, height: number, devicePixelRatio: number): void;
  setTheme(theme: ViewportTheme): void;
  command(command: ViewCommand): void;
  setTool(tool: Tool): void;
  onEvent(listener: (event: ViewportEvent) => void): void;
}

/** The loaded viewport module. */
export interface ViewportModule {
  webgpuAvailable(): Promise<boolean>;
  create(canvas: HTMLCanvasElement, options: ViewportOptions): Promise<ViewportHandle>;
}
