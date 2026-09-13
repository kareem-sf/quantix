import { fireEvent, render, screen } from "@testing-library/react";
import { ApiContext, createApi } from "../api";
import { GenerationControls } from "./GenerationControls";

const settings = {
  max_output_tokens: 8192,
  output_mode: "auto" as const,
  native_tools: [],
  max_search_calls: 3,
  max_native_tool_calls: 3,
};

it("keeps advanced controls in More options and preserves unrelated preferences", () => {
  const change = vi.fn();
  const view = render(
    <GenerationControls value={settings} onChange={change} />,
  );
  expect(screen.getByText("More options")).toBeInTheDocument();
  fireEvent.click(screen.getByText("More options"));
  fireEvent.change(screen.getByLabelText("Temperature"), {
    target: { value: "0.4" },
  });
  expect(change).toHaveBeenCalledWith({ ...settings, temperature: 0.4 });
  view.rerender(
    <GenerationControls
      value={{ ...settings, temperature: 0.4 }}
      onChange={change}
    />,
  );
  fireEvent.change(screen.getByLabelText("Temperature"), {
    target: { value: "" },
  });
  expect(change).toHaveBeenLastCalledWith({ ...settings, temperature: null });
});

it("makes hosted code limitations visible and keeps settings editable without an account", () => {
  render(<GenerationControls value={settings} onChange={vi.fn()} />);
  fireEvent.click(screen.getByText("More options"));
  expect(screen.getByLabelText("Provider code execution")).toBeEnabled();
  expect(screen.getByText(/Choose an account and model/)).toBeInTheDocument();
});

it("shows an exact model rejection and removes the stale preview when selection changes", async () => {
  const fetcher = vi.fn(async () =>
    Response.json({
      connection_id: "account",
      model_id: "exact",
      connection_revision: 1,
      revision: "capability-revision",
      requested: settings,
      effective: null,
      capabilities: [],
      blockers: ["Temperature is unknown for this model."],
    }),
  );
  const api = createApi(
    { base_url: "http://localhost/api", token: "fixture" },
    fetcher,
  );
  const content = (model: string) => (
    <ApiContext.Provider value={api}>
      <GenerationControls
        value={settings}
        onChange={vi.fn()}
        connectionId="account"
        modelId={model}
      />
    </ApiContext.Provider>
  );
  const view = render(content("exact"));
  fireEvent.click(screen.getByText("More options"));
  fireEvent.click(screen.getByRole("button", { name: "Check settings" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Temperature is unknown for this model.",
  );
  expect(fetcher.mock.calls).toHaveLength(1);
  view.rerender(content("other"));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});
