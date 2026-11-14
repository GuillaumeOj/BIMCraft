import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";
import type { ToolStep, ViewCommand, ViewState } from "./types";

/** What the 3D view reports for the View panel; the panel renders only this (UI-01). */
export type ViewReport = {
  /** Null until the viewport sent its first state, and after it stopped. */
  view: ViewState | null;
  step: ToolStep;
  selectionCount: number;
  /** How many faces of the section box the user moved since it was switched on or reset. */
  facesMoved: number;
  send: (command: ViewCommand) => void;
};

const IDLE: ViewReport = {
  view: null,
  step: "idle",
  selectionCount: 0,
  facesMoved: 0,
  send: () => undefined,
};

type Bridge = { report: ViewReport; publish: (report: ViewReport) => void };

const BridgeContext = createContext<Bridge>({ report: IDLE, publish: () => undefined });

/**
 * Carries the 3D view's report to the View panel, which sits beside the view in the shell
 * rather than inside it. Without a provider (the view alone in a test) the report goes nowhere.
 */
export function ViewportBridgeProvider({ children }: { children: ReactNode }) {
  const [report, setReport] = useState(IDLE);
  const publish = useCallback((next: ViewReport) => setReport(next), []);
  const value = useMemo(() => ({ report, publish }), [report, publish]);
  return <BridgeContext.Provider value={value}>{children}</BridgeContext.Provider>;
}

/** The latest report of the 3D view. */
export const useViewReport = (): ViewReport => useContext(BridgeContext).report;

/** Where the 3D view puts its report. */
export const usePublishViewReport = (): Bridge["publish"] => useContext(BridgeContext).publish;

/** The report of a view that is not running. */
export const IDLE_REPORT = IDLE;
