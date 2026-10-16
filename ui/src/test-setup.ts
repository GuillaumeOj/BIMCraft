import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Spies are restored by restoreMocks (vitest.config.ts); the stored language choice is reset here.
afterEach(() => {
  cleanup();
  localStorage.clear();
});
