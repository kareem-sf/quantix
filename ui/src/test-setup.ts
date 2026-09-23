import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach, vi } from "vitest";

configure({ asyncUtilTimeout: 5000 }); // findBy and waitFor wait longer on a busy machine

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
