import type { ReactNode } from "react";

/** A message about the whole form: announced at once when it is an error. */
export function Banner({ children }: { children: ReactNode }) {
  return (
    <p className="banner" role="alert">
      {children}
    </p>
  );
}
