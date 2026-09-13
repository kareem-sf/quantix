import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Measurements } from "./Measurements";

const source = {
  artifact_id: "drawing",
  version: 1,
  content_hash: "a".repeat(64),
  name: "Plan.pdf",
  relative_path: "Drawings/Plan.pdf",
  page: 1,
  page_count: 2,
  page_size: [200, 100],
  is_current: true,
};
const calculation = {
  source,
  mode: "length",
  points: [
    [0, 0],
    [0, 0.6],
  ],
  calibration_points: [
    [0, 0],
    [0.2, 0],
  ],
  calibration_metres: "4",
  quantity: "6",
  unit: "m",
  calculation: "Sum of segment lengths using the stated calibration.",
  calculation_version: "calibrated-pdf-v1",
  coordinate_system: "normalized_top_left",
  precision_note: "Review the calibration and written dimensions before use.",
};

function setup(
  options: {
    previewError?: boolean;
    calculate?: () => Promise<Response>;
    initialPage?: number;
  } = {},
) {
  const writes: { path: string; body: Record<string, unknown> }[] = [];
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      const url = new URL(String(address)),
        path = url.pathname;
      if (init?.method === "POST") {
        const body = JSON.parse(String(init.body));
        writes.push({ path, body });
        if (path.endsWith("/calculate"))
          return options.calculate
            ? options.calculate()
            : Response.json(calculation);
        return Response.json({
          ...calculation,
          id: "saved",
          tender_id: "tender",
          source_id: "derived",
          scope_label: body.scope_label,
          status: "proposed",
          origin: "engineer",
          reviewed_at: "2026-09-06",
          review_rationale: body.rationale,
          is_current: true,
          created_at: "2026-09-06",
          links: [],
        });
      }
      if (path.endsWith("/measurement-page"))
        return Response.json({
          ...source,
          page: Number(url.searchParams.get("page")),
        });
      if (path.endsWith("/preview"))
        return options.previewError
          ? Response.json(
              { detail: "The page could not be opened." },
              { status: 400 },
            )
          : new Response(new Blob(["PNG"], { type: "image/png" }));
      if (path.endsWith("/measurements")) return Response.json([]);
      if (path.endsWith("/estimate")) return Response.json({ items: [] });
      throw new Error(`Unexpected request ${path}`);
    },
  );
  const view = render(
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <Measurements
          tenderId="tender"
          artifactId="drawing"
          initialPage={options.initialPage}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { ...view, writes, client };
}

beforeEach(() => {
  window.localStorage.clear();
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL() {
        return "blob:actual-preview";
      }
      static revokeObjectURL() {}
    },
  );
});

test("opens the requested source page and keeps its revision in the measurement basis", async () => {
  setup({ initialPage: 2 });
  expect(
    await screen.findByRole("img", { name: "Plan.pdf, page 2" }),
  ).toBeInTheDocument();
  expect(screen.getByText(/Page 2 of 2/)).toBeInTheDocument();
});
afterEach(() => vi.unstubAllGlobals());

async function loadDrawing() {
  const image = await screen.findByRole("img", { name: "Plan.pdf, page 1" });
  fireEvent.load(image);
  const overlay = screen.getByRole("img", { name: "Measurement overlay" });
  vi.spyOn(overlay, "getBoundingClientRect").mockReturnValue({
    x: 10,
    y: 20,
    left: 10,
    top: 20,
    right: 410,
    bottom: 220,
    width: 400,
    height: 200,
    toJSON() {},
  });
  return overlay;
}

async function markAndCalculate() {
  const overlay = await loadDrawing();
  fireEvent.click(overlay, { clientX: 10, clientY: 20 });
  fireEvent.click(overlay, { clientX: 90, clientY: 20 });
  await userEvent.type(screen.getByLabelText("Known length (m)"), "4");
  await userEvent.click(
    screen.getByRole("button", { name: "Mark measurement" }),
  );
  fireEvent.click(overlay, { clientX: 10, clientY: 20 });
  fireEvent.click(overlay, { clientX: 10, clientY: 140 });
  await userEvent.click(
    screen.getByRole("button", { name: "Calculate quantity" }),
  );
}

test("uses the displayed image bounds for overlays and never sends page dimensions", async () => {
  const { writes } = setup();
  await markAndCalculate();
  expect(await screen.findByText("6 m")).toBeInTheDocument();
  expect(writes[0].body.points).toEqual([
    [0, 0],
    [0, 0.6],
  ]);
  expect(writes[0].body.calibration_points).toEqual([
    [0, 0],
    [0.2, 0],
  ]);
  expect(writes[0].body).not.toHaveProperty("page_size");
  expect(
    screen.getByRole("img", { name: "Measurement overlay" }),
  ).toHaveAttribute("viewBox", "0 0 200 100");
  expect(
    screen.getByRole("button", { name: "Save reviewed proposal" }),
  ).toBeDisabled();
  await userEvent.type(
    screen.getByLabelText("Measurement scope"),
    "East boundary",
  );
  await userEvent.type(
    screen.getByLabelText("Review note"),
    "Checked the written dimension.",
  );
  await userEvent.click(screen.getByLabelText(/I reviewed the calibration/));
  await userEvent.click(
    screen.getByRole("button", { name: "Save reviewed proposal" }),
  );
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[1].body).toMatchObject({
    engineer_confirmed: true,
    scope_label: "East boundary",
    rationale: "Checked the written dimension.",
  });
  expect(
    await screen.findByText(/Saved measurement proposal/),
  ).toBeInTheDocument();
});

test("cancelled calculations cannot restore a cancelled draft or save it", async () => {
  let finish: (response: Response) => void = () => {};
  const { writes } = setup({
    calculate: () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  });
  await markAndCalculate();
  await userEvent.click(
    screen.getByRole("button", { name: "Cancel measurement" }),
  );
  finish(Response.json(calculation));
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Calculate quantity" }),
    ).toBeDisabled(),
  );
  expect(screen.queryByText("6 m")).not.toBeInTheDocument();
  expect(writes).toHaveLength(1);
});

test("failed source preview exposes the error and prevents marking and calculation", async () => {
  setup({ previewError: true });
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The page could not be opened",
  );
  expect(
    screen.queryByRole("img", { name: "Measurement overlay" }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Calculate quantity" }),
  ).not.toBeInTheDocument();
});

test("changing pages clears calibration and result so points cannot move to another page", async () => {
  setup();
  await markAndCalculate();
  await screen.findByText("6 m");
  await userEvent.click(
    screen.getByRole("button", { name: "Next drawing page" }),
  );
  expect(
    await screen.findByRole("img", { name: "Plan.pdf, page 2" }),
  ).toBeInTheDocument();
  expect(screen.getByLabelText("Known length (m)")).toHaveValue(null);
  expect(screen.queryByText("6 m")).not.toBeInTheDocument();
});

test("calculation errors keep the marked draft available for correction", async () => {
  setup({
    calculate: async () =>
      Response.json(
        { detail: "The area boundary crosses itself." },
        { status: 400 },
      ),
  });
  await markAndCalculate();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The area boundary crosses itself.",
  );
  expect(screen.getByRole("button", { name: "Undo point" })).toBeEnabled();
  expect(
    screen.queryByRole("button", { name: "Save reviewed proposal" }),
  ).not.toBeInTheDocument();
});

test("refreshing unchanged source data preserves unsaved measurement marks", async () => {
  const objectUrls = vi.spyOn(URL, "createObjectURL");
  const { client } = setup();
  const overlay = await loadDrawing();
  await userEvent.selectOptions(
    screen.getByLabelText("Measurement type"),
    "count",
  );
  fireEvent.click(overlay, { clientX: 90, clientY: 100 });
  const previews = objectUrls.mock.calls.length;
  await act(async () => {
    await client.invalidateQueries();
  });
  await waitFor(() =>
    expect(objectUrls.mock.calls.length).toBeGreaterThan(previews),
  );
  expect(screen.getByLabelText("Measurement type")).toHaveValue("count");
  expect(
    screen.getByRole("button", { name: "Calculate quantity" }),
  ).toBeEnabled();
});
