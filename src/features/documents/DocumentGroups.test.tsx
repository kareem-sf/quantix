import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../../api";
import { DocumentGroups } from "./DocumentGroups";

const doc = (id: string, name: string, extra = {}) => ({
  artifact_id: id,
  name,
  label: "",
  pages: null,
  kind: "pdf",
  problem: "",
  moved_by_engineer: false,
  ...extra,
});

it("shows AI groups with English labels, flags only problems, and moves a file", async () => {
  const user = userEvent.setup();
  const onOpen = vi.fn();
  const puts: unknown[] = [];
  let state = {
    grouped: true,
    total: 3,
    problems: 1,
    groups: [
      {
        name: "Drawings",
        documents: [
          doc("elec", "FIRE STATION-ELEC.pdf", {
            label: "Electrical layouts and schedules",
            pages: 14,
          }),
        ],
      },
      {
        name: "Bill of quantities",
        documents: [
          doc("boq", "1-جدول الكميات.pdf", {
            label: "Priced BOQ schedule (Arabic)",
            pages: 17,
          }),
        ],
      },
      {
        name: "Site visit",
        documents: [
          doc("form", "13.نموذج الزيارة.pdf", {
            problem: "Arabic text recognition is not installed.",
          }),
        ],
      },
    ],
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "PUT") {
        puts.push(JSON.parse(String(init.body)));
        state = {
          ...state,
          groups: [
            {
              name: "Drawings",
              documents: [
                state.groups[0].documents[0],
                { ...state.groups[1].documents[0], moved_by_engineer: true },
              ],
            },
            state.groups[2],
          ],
        };
      }
      return Response.json(state);
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <DocumentGroups tenderId="one" onOpen={onOpen} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const drawings = await screen.findByRole("region", { name: "Drawings" });
  expect(drawings).toHaveTextContent(
    "Electrical layouts and schedules · 14 pp",
  );
  expect(screen.queryByText(/Version 1|Current/)).not.toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Site visit" })).toHaveTextContent(
    "Arabic text recognition is not installed.",
  );

  await user.click(
    within(drawings).getByRole("button", { name: /^FIRE STATION-ELEC\.pdf/ }),
  );
  expect(onOpen).toHaveBeenCalledWith("elec");

  const boq = screen.getByRole("region", { name: "Bill of quantities" });
  await user.click(
    within(boq).getByRole("button", { name: /Move .* to another group/ }),
  );
  await user.click(within(boq).getByRole("button", { name: "Move" }));
  expect(puts).toEqual([{ group: "Drawings" }]);
  expect(
    await screen.findByRole("region", { name: "Drawings" }),
  ).toHaveTextContent("Priced BOQ schedule (Arabic)");
});

it("stops saying it is sorting once the grouping run ends", async () => {
  const user = userEvent.setup();
  const state = {
    grouped: false,
    total: 1,
    problems: 0,
    groups: [{ name: "Not grouped yet", documents: [doc("a", "Spec.pdf")] }],
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const path = new URL(String(url)).pathname;
      if (init?.method === "POST") return Response.json({ id: "r1" });
      if (path === "/api/runs/r1")
        return Response.json({
          id: "r1",
          status: "failed",
          error: "The AI service dropped its reply.",
        });
      return Response.json(state);
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <DocumentGroups tenderId="t" onOpen={() => undefined} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Group documents now" }),
  );
  expect(
    await screen.findByText(
      "Grouping stopped: The AI service dropped its reply.",
    ),
  ).toBeInTheDocument();
  expect(screen.queryByText(/Sorting the documents/)).not.toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Group documents now" }),
  ).toBeInTheDocument();
});
