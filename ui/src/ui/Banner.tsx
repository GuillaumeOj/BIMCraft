import type { ReactNode } from "react";
import { ErrorIcon } from "./ErrorIcon";

/** A message about the whole form: announced at once when it is an error. */
export function Banner({ children }: { children: ReactNode }) {
  return (
    <p className="banner" role="alert">
      <ErrorIcon />
      <span>{children}</span>
    </p>
  );
}
