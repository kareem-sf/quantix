import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { SandboxSettings } from "./SandboxSettings";

function mount(state = "prerequisite_required") {
  const writes: unknown[] = [];
  const data = {
    state,
    detail: "Windows Subsystem for Linux 2 is not ready.",
    next_action: "Complete WSL 2 setup in Windows.",
    composition_available: true,
    python_available: state === "ready",
    active_runs: 0,
    image_id: null,
    library_versions: {},
    limits: {
      cpus: 2,
      memory_mib: 2048,
      seconds: 120,
      processes: 64,
      output_mib: 50,
    },
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (init?.method !== "GET") writes.push(JSON.parse(String(init?.body)));
      return new Response(JSON.stringify(data));
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <SandboxSettings />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { writes, user: userEvent.setup({ delay: null }) };
}

it("shows the Windows prerequisite and checks again only on engineer action", async () => {
  const { writes, user } = mount();
  expect(
    await screen.findByText("Windows Subsystem for Linux 2 is not ready."),
  ).toBeInTheDocument();
  expect(writes).toEqual([]);
  await user.click(
    screen.getByRole("button", { name: "Check Windows setup again" }),
  );
  expect(writes).toEqual([{ action: "setup" }]);
});

it("requires the explicit remove action before deleting the private machine", async () => {
  const { writes, user } = mount("ready");
  await screen.findByText("Local calculations and Python");
  await user.click(screen.getByText("More options"));
  await user.click(screen.getByRole("button", { name: "Remove runtime" }));
  expect(writes).toEqual([]);
  await user.click(
    screen.getByRole("button", { name: "Remove private machine" }),
  );
  expect(writes).toEqual([{ action: "remove" }]);
});
