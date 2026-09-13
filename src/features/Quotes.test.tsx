import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { Quotes } from "./Quotes";

const fingerprint = "a".repeat(64);
const original: Schema<"QuoteRecord"> = {
  id: "quote-one",
  tender_id: "one",
  to: ["supplier@example.com"],
  cc: ["engineer@example.com"],
  subject: "Concrete quotation",
  body: "Please quote the attached concrete works.\n",
  attachment_ids: [],
  source_ids: [],
  message_id: "<quote-one@quantix.local>",
  status: "draft",
  attachments: [],
  approved_fingerprint: null,
  delivery_detail: "",
  refused_recipients: [],
  created_at: "2026-09-06T10:00:00Z",
  updated_at: "2026-09-06T10:00:00Z",
};
const artifact: Schema<"Artifact"> = {
  id: "artifact-one",
  tender_id: "one",
  name: "BOQ.xlsx",
  relative_path: "Civil/BOQ.xlsx",
  version: 2,
  content_hash: "b".repeat(64),
  size: 1500,
  kind: "spreadsheet",
  status: "extracted",
  area: "Civil",
  is_current: true,
  created_at: original.created_at,
};

function setup(
  options: {
    quote?: Partial<Schema<"QuoteRecord">>;
    sendError?: string;
    smtpReady?: boolean;
    restoreRequired?: boolean;
    artifacts?: Schema<"Artifact">[];
  } = {},
) {
  let quote = { ...original, ...options.quote };
  const writes: {
    path: string;
    method: string;
    body: Record<string, unknown>;
  }[] = [];
  const replies: Schema<"ReplyRecord">[] = [];
  const reads: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const path = String(url).replace("http://localhost/api", "");
      if (init?.method === "POST" || init?.method === "PATCH") {
        const body = JSON.parse(String(init.body));
        writes.push({ path, method: init.method, body });
        if (path.endsWith("/send") && options.sendError)
          return new Response(JSON.stringify({ detail: options.sendError }), {
            status: 400,
          });
        if (path.endsWith("/approve"))
          quote = {
            ...quote,
            status: "approved",
            approved_fingerprint: body.fingerprint,
          };
        else if (path.endsWith("/send"))
          quote = {
            ...quote,
            status: "sent",
            delivery_detail:
              "SMTP accepted all recipients. Inbox delivery is not confirmed.",
          };
        else if (path.endsWith("/replies")) {
          const reply: Schema<"ReplyRecord"> = {
            ...body,
            id: "reply-one",
            quote_id: quote.id,
            tender_id: "one",
            origin: "manual",
            date_basis: "engineer_entered",
            message_id: null,
            source_ids: ["registered-source"],
            supporting_source_ids: body.source_ids,
            warnings: [],
            created_at: original.created_at,
          };
          replies.push(reply);
          return new Response(JSON.stringify(reply));
        } else quote = { ...quote, ...body };
        return new Response(JSON.stringify(quote));
      }
      reads.push(path);
      if (path.endsWith("/eml"))
        return new Response(
          "From: tenders@example.com\r\nSubject: Concrete quotation\r\n\r\nRequest body",
          { headers: { "Content-Type": "message/rfc822" } },
        );
      const data = path.endsWith("/preview")
        ? {
            quote,
            sender: "tenders@example.com",
            attachments: quote.attachments,
            fingerprint,
            smtp_host: "smtp.example.com",
            smtp_ready: options.smtpReady ?? true,
            warnings: [],
            restore_reconciliation_required: options.restoreRequired ?? false,
          }
        : path.endsWith("/replies")
          ? replies
          : path.endsWith("/artifacts")
            ? (options.artifacts ?? [artifact])
            : path.includes("/evidence/")
              ? {
                  id: "registered-source",
                  artifact_name: "Supplier reply",
                  locator: "Reply text",
                  relative_path: "mail/reply.json",
                }
              : [quote];
      return new Response(JSON.stringify(data));
    },
  );
  const onSource = vi.fn();
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Quotes tenderId="one" onSource={onSource} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { user: userEvent.setup({ delay: null }), writes, reads, onSource };
}

async function review(user: ReturnType<typeof userEvent.setup>) {
  await user.click(
    await screen.findByRole("button", {
      name: "Review request: Concrete quotation",
    }),
  );
  await screen.findByText("tenders@example.com");
}

it("creates a draft with selected Tender artifact IDs and no approval", async () => {
  const { user, writes } = setup();
  await user.click(
    await screen.findByRole("button", { name: "New quotation request" }),
  );
  await fill(user, screen.getByLabelText("To"), "new@example.com");
  await fill(user, screen.getByLabelText("Subject"), "Reinforcement quotation");
  await fill(
    user,
    screen.getByLabelText("Message"),
    "Please quote reinforcement.",
  );
  await user.click(await screen.findByLabelText(/Civil\/BOQ.xlsx/));
  await user.click(screen.getByRole("button", { name: "Save draft" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toEqual({
    path: "/tenders/one/quotes",
    method: "POST",
    body: {
      to: ["new@example.com"],
      cc: [],
      subject: "Reinforcement quotation",
      body: "Please quote reinforcement.",
      attachment_ids: ["artifact-one"],
      source_ids: [],
    },
  });
});

it("filters attachment paths case-insensitively without losing hidden selections", async () => {
  const other = {
    ...artifact,
    id: "roof-file",
    name: "Roof.pdf",
    relative_path: "Architecture/Roof.pdf",
    kind: "pdf",
  };
  const { user, writes } = setup({ artifacts: [artifact, other] });
  await user.click(
    await screen.findByRole("button", { name: "New quotation request" }),
  );
  await fill(user, screen.getByLabelText("To"), "supplier@example.com");
  await fill(user, screen.getByLabelText("Subject"), "Review attachments");
  await fill(
    user,
    screen.getByLabelText("Message"),
    "Please review the selected documents.",
  );
  await user.click(await screen.findByLabelText(/Civil\/BOQ.xlsx/));
  await fill(
    user,
    screen.getByLabelText("Filter attachment files"),
    "ARCHITECTURE/ROOF",
  );
  expect(screen.queryByLabelText(/Civil\/BOQ.xlsx/)).not.toBeInTheDocument();
  expect(screen.getByText(/1 selected/)).toBeInTheDocument();
  await user.click(screen.getByLabelText(/Architecture\/Roof.pdf/));
  await user.clear(screen.getByLabelText("Filter attachment files"));
  await fill(
    user,
    screen.getByLabelText("Filter attachment files"),
    "no matching file",
  );
  expect(screen.getByText("No files match this filter.")).toBeInTheDocument();
  expect(screen.getByText(/2 selected/)).toBeInTheDocument();
  await user.clear(screen.getByLabelText("Filter attachment files"));
  expect(screen.getByLabelText(/Civil\/BOQ.xlsx/)).toBeChecked();
  expect(screen.getByLabelText(/Architecture\/Roof.pdf/)).toBeChecked();
  await user.click(screen.getByRole("button", { name: "Save draft" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0].body.attachment_ids).toEqual(["artifact-one", "roof-file"]);
});

it("uses one reviewed fingerprint and decision for approve and send", async () => {
  const { user, writes } = setup();
  await review(user);
  expect(screen.getByText(original.body.trim())).toBeInTheDocument();
  expect(screen.getByText("supplier@example.com")).toBeInTheDocument();
  const send = screen.getByRole("button", { name: "Approve and send" });
  expect(send).toBeDisabled();
  await fill(
    user,
    screen.getByLabelText("Approval note"),
    "Request prices for the approved package.",
  );
  await user.click(screen.getByLabelText(/I have reviewed the recipients/));
  await user.click(send);
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes.map((item) => item.path)).toEqual([
    "/tenders/one/quotes/quote-one/approve",
    "/tenders/one/quotes/quote-one/send",
  ]);
  expect(writes[0].body).toEqual({
    fingerprint,
    engineer_confirmed: true,
    rationale: "Request prices for the approved package.",
  });
  expect(writes[1].body).toEqual(writes[0].body);
  expect(
    await screen.findByText(/Inbox delivery is not confirmed/),
  ).toBeInTheDocument();
});

it("permits approval without mail configuration and never submits it", async () => {
  const { user, writes } = setup({ smtpReady: false });
  await review(user);
  await fill(
    user,
    screen.getByLabelText("Approval note"),
    "Approved wording for review.",
  );
  await user.click(screen.getByLabelText(/I have reviewed the recipients/));
  expect(
    screen.getByRole("button", { name: "Approve and send" }),
  ).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Approve only" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0].path).toMatch(/\/approve$/);
});

it("preserves newer delivery history and blocks sending an older restored draft", async () => {
  const { user, writes } = setup({
    quote: {
      status: "approved",
      delivery_history_status: "sent",
      delivery_history_fingerprint: "b".repeat(64),
    },
  });
  await review(user);
  expect(screen.getAllByText(/Local delivery history: Sent/)).toHaveLength(2);
  expect(
    screen.getByText(/wording differs from the recorded submission/),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Approve and send" }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Edit draft" }),
  ).not.toBeInTheDocument();
  expect(writes).toHaveLength(0);
});

it("shows send errors without retrying and requires fresh review", async () => {
  const { user, writes } = setup({
    sendError: "The connection closed before acceptance could be established.",
  });
  await review(user);
  await fill(user, screen.getByLabelText("Approval note"), "Approved scope.");
  await user.click(screen.getByLabelText(/I have reviewed the recipients/));
  await user.click(screen.getByRole("button", { name: "Approve and send" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The connection closed",
  );
  expect(writes).toHaveLength(2);
  expect(
    screen.getByRole("button", { name: "Approve and send" }),
  ).toBeDisabled();
});

it("registers a dated manual reply and opens the resulting evidence", async () => {
  const { user, writes, onSource } = setup();
  await review(user);
  await user.click(
    screen.getByRole("button", { name: "Register supplier reply" }),
  );
  await fill(
    user,
    screen.getByLabelText("Supplier email"),
    "supplier@example.com",
  );
  await fill(
    user,
    screen.getByLabelText("Received date and time with time zone"),
    "2026-09-06T13:00:00+03:00",
  );
  await fill(
    user,
    screen.getByLabelText("Reply text"),
    "Concrete rate is subject to delivery distance.",
  );
  await user.click(screen.getByRole("button", { name: "Save supplier reply" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0].body).toMatchObject({
    sender: "supplier@example.com",
    received_at: "2026-09-06T13:00:00+03:00",
    source_ids: [],
  });
  await user.click(
    await screen.findByRole("button", { name: "Supplier reply · Reply text" }),
  );
  expect(onSource).toHaveBeenCalledWith({ sourceId: "registered-source" });
});

it("requires an engineer-written restore reconciliation before sending", async () => {
  const { user, writes } = setup({ restoreRequired: true });
  await review(user);
  await fill(user, screen.getByLabelText("Approval note"), "Approved scope.");
  await user.click(screen.getByLabelText(/I have reviewed the recipients/));
  expect(
    screen.getByRole("button", { name: "Approve and send" }),
  ).toBeDisabled();
  const note =
    "Checked the sent mailbox and supplier; this request was not already submitted.";
  await fill(
    user,
    screen.getByLabelText("Restore delivery reconciliation"),
    note,
  );
  await user.click(screen.getByRole("button", { name: "Approve and send" }));
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[0].body.restore_reconciliation).toBe(note);
  expect(writes[1].body).toEqual(writes[0].body);
});

it("edits the full draft without retaining approval fields", async () => {
  const { user, writes } = setup({
    quote: {
      status: "approved",
      approved_fingerprint: fingerprint,
      source_ids: ["registered-source"],
    },
  });
  await review(user);
  await user.click(screen.getByRole("button", { name: "Edit draft" }));
  await user.clear(screen.getByLabelText("Subject"));
  await fill(
    user,
    screen.getByLabelText("Subject"),
    "Revised concrete quotation",
  );
  await user.click(screen.getByRole("button", { name: "Save draft" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toEqual({
    path: "/tenders/one/quotes/quote-one",
    method: "PATCH",
    body: {
      to: original.to,
      cc: original.cc,
      subject: "Revised concrete quotation",
      body: original.body,
      attachment_ids: [],
      source_ids: ["registered-source"],
    },
  });
});

it("downloads the real EML through the authenticated API without sending", async () => {
  const { user, writes, reads } = setup();
  const create = vi.fn((_blob: Blob) => "blob:test-mail"),
    revoke = vi.fn();
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL = create;
      static revokeObjectURL = revoke;
    },
  );
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(() => {});
  try {
    await review(user);
    await user.click(screen.getByRole("button", { name: "Download EML" }));
    await waitFor(() => expect(create).toHaveBeenCalledTimes(1));
    expect(reads).toContain("/tenders/one/quotes/quote-one/eml");
    expect(create.mock.calls[0][0]).toBeInstanceOf(Blob);
    expect(click).toHaveBeenCalledTimes(1);
    expect(writes).toHaveLength(0);
  } finally {
    click.mockRestore();
    vi.unstubAllGlobals();
  }
});

it("does not accept manual reply dates without a time zone", async () => {
  const { user, writes } = setup();
  await review(user);
  await user.click(
    screen.getByRole("button", { name: "Register supplier reply" }),
  );
  await fill(
    user,
    screen.getByLabelText("Supplier email"),
    "supplier@example.com",
  );
  await fill(
    user,
    screen.getByLabelText("Received date and time with time zone"),
    "2026-09-06T13:00:00",
  );
  await fill(user, screen.getByLabelText("Reply text"), "Quotation follows.");
  await user.click(screen.getByRole("button", { name: "Save supplier reply" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "including its time zone",
  );
  expect(writes).toHaveLength(0);
});

it("limits selection to 20 current Tender attachments", async () => {
  const files = Array.from({ length: 21 }, (_, index) => ({
    ...artifact,
    id: `file-${index}`,
    relative_path: `Civil/BOQ-${index}.xlsx`,
  }));
  const { user } = setup({
    artifacts: [
      ...files,
      {
        ...artifact,
        id: "old-file",
        is_current: false,
        relative_path: "Superseded.xlsx",
      },
    ],
  });
  await user.click(
    await screen.findByRole("button", { name: "New quotation request" }),
  );
  for (let index = 0; index < 20; index++)
    await user.click(
      await screen.findByLabelText(new RegExp(`Civil/BOQ-${index}\\.xlsx`)),
    );
  expect(screen.getByLabelText(/Civil\/BOQ-20.xlsx/)).toBeDisabled();
  expect(screen.queryByText(/Superseded.xlsx/)).not.toBeInTheDocument();
});

it("opens supporting evidence from an edit form without saving the draft", async () => {
  const { user, writes, onSource } = setup({
    quote: { source_ids: ["registered-source"] },
  });
  await review(user);
  await user.click(screen.getByRole("button", { name: "Edit draft" }));
  await user.click(screen.getByText("Supporting source references (1)"));
  await user.click(
    await screen.findByRole("button", { name: "Supplier reply · Reply text" }),
  );
  expect(onSource).toHaveBeenCalledWith({ sourceId: "registered-source" });
  expect(writes).toHaveLength(0);
});

async function fill(
  user: ReturnType<typeof userEvent.setup>,
  input: HTMLElement,
  value: string,
) {
  await user.click(input);
  await user.paste(value);
}

it("restores an unsent quotation draft after leaving its editor without approving or sending", async () => {
  const { user, writes } = setup();
  await user.click(
    screen.getByRole("button", { name: "New quotation request" }),
  );
  await fill(user, screen.getByLabelText("To"), "draft@example.com");
  await fill(user, screen.getByLabelText("Subject"), "Pending review");
  await fill(
    user,
    screen.getByLabelText("Message"),
    "Keep my unsent scope note.",
  );
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  await user.click(
    screen.getByRole("button", { name: "New quotation request" }),
  );
  expect(screen.getByLabelText("Subject")).toHaveValue("Pending review");
  expect(screen.getByLabelText("Message")).toHaveValue(
    "Keep my unsent scope note.",
  );
  expect(writes).toEqual([]);
});
