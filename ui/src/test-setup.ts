import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import { forgetStoredInMemory } from "./ui/storage";

// Spies are restored by restoreMocks (vitest.config.ts); stored choices (the language, the
// dismissed navigation hint) are reset here.
afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
  forgetStoredInMemory();
});
