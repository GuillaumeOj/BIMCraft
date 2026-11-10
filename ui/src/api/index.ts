// The API client: UI code imports from here only (UI-01). Biome forbids Tauri elsewhere.
export { ApiProvider, useApi } from "./ApiContext";
export {
  BUILDUP_FUNCTIONS,
  type BuildUpDetail,
  type BuildUpPreview,
  type BuildUpSummary,
  buildUpRefusal,
  createBuildUp,
  deleteBuildUp,
  duplicateBuildUp,
  getBuildUp,
  listBuildUps,
  MAX_LAYERS,
  previewBuildUp,
  updateBuildUp,
} from "./buildups";
export { ApiClient, type Transport } from "./client";
export { apiReason, type GeoreferenceValue, newEntityId, setGeoreference } from "./georef";
export { ApiError, TransportError } from "./jsonrpc";
export {
  addFromLibrary,
  createMaterial,
  deleteMaterial,
  type GradeDetail,
  type GradeSummary,
  getGrade,
  getMaterial,
  historyDepth,
  type LibraryPage,
  listMaterials,
  MATERIAL_CATEGORIES,
  type MaterialDetail,
  type MaterialFilter,
  type MaterialRefusal,
  type MaterialSummary,
  materialRefusal,
  renameMaterial,
  searchLibrary,
  setPropertyValue,
  undoLastStep,
} from "./materials";
export type * from "./protocol.gen";
export { tauriTransport } from "./tauri";
