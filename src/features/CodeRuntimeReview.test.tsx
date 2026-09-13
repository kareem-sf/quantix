import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import type { Schema } from "../api";
import { CodeRuntimeReview } from "./CodeRuntimeReview";

const monty: Schema<"ReviewedCodeRuntime"> = {
  engine: "monty",
  fingerprint: "a".repeat(64),
  version: "0.9.3",
  image_id: null,
  library_versions: { monty: "0.9.3" },
  seconds: 8,
  memory_mib: 64,
  cpus: 1,
  processes: 1,
  output_mib: 2,
  tool_calls: 4,
};

const python: Schema<"ReviewedCodeRuntime"> = {
  engine: "python",
  fingerprint: "b".repeat(64),
  version: "python-3.13",
  image_id: "sha256:" + "c".repeat(64),
  library_versions: { numpy: "2.3.2", pandas: "2.3.2" },
  seconds: 45,
  memory_mib: 1024,
  cpus: 2,
  processes: 32,
  output_mib: 20,
  tool_calls: 6,
};

it("selects an exact runtime and requests its fixed execution tool", async () => {
  const user = userEvent.setup();
  const changed = vi.fn();
  render(
    <CodeRuntimeReview
      options={[monty, python]}
      value={[]}
      disabled={false}
      onChange={changed}
    />,
  );

  const option = screen.getByRole("group", {
    name: "Monty calculation sandbox",
  });
  expect(within(option).getByText("Version 0.9.3")).toBeVisible();
  expect(within(option).getByText(/8 seconds.*64 MiB.*4 calls/)).toBeVisible();
  await user.click(within(option).getByRole("checkbox"));

  expect(changed).toHaveBeenCalledWith([monty], ["execute_tool_code"]);
});

it("allows only lower resource ceilings in More options", async () => {
  const user = userEvent.setup();
  const changed = vi.fn();
  render(
    <CodeRuntimeReview
      options={[python]}
      value={[python]}
      disabled={false}
      onChange={changed}
    />,
  );

  await user.click(screen.getByText("More options"));
  const seconds = screen.getByRole("spinbutton", { name: "Maximum run time" });
  expect(seconds).toHaveAttribute("max", "45");
  await user.clear(seconds);
  await user.type(seconds, "20");
  await user.tab();

  expect(changed).toHaveBeenLastCalledWith(
    [expect.objectContaining({ engine: "python", seconds: 20 })],
    [],
  );
  expect(screen.getByText(python.fingerprint)).toBeVisible();
  expect(screen.getByText(python.image_id!)).toBeVisible();
});

it("keeps a historical selected proof when the current runtime has changed", () => {
  const historical = {
    ...python,
    fingerprint: "d".repeat(64),
    version: "python-3.12",
    image_id: "sha256:" + "e".repeat(64),
    seconds: 20,
  } satisfies Schema<"ReviewedCodeRuntime">;
  render(
    <CodeRuntimeReview
      options={[python]}
      value={[historical]}
      disabled
      onChange={vi.fn()}
    />,
  );

  expect(screen.getByText("Version python-3.12")).toBeVisible();
  expect(screen.getByText(historical.fingerprint)).toBeInTheDocument();
  expect(screen.queryByText(python.fingerprint)).not.toBeInTheDocument();
});
