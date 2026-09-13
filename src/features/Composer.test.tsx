import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { Composer } from "./Composer";

it("restores an unsent draft after remount without exposing it in another tender", async () => {
  const user = userEvent.setup();
  let view = render(
    <Composer
      tenderId="draft-one"
      onSend={async () => {}}
      onImport={() => {}}
    />,
  );
  await user.type(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
    "Check the revised drainage quantities",
  );
  view.unmount();
  view = render(
    <Composer
      tenderId="draft-two"
      onSend={async () => {}}
      onImport={() => {}}
    />,
  );
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("");
  view.unmount();
  render(
    <Composer
      tenderId="draft-one"
      onSend={async () => {}}
      onImport={() => {}}
    />,
  );
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("Check the revised drainage quantities");
});

it("does not restore an accepted message as a new unsent draft", async () => {
  const user = userEvent.setup();
  const view = render(
    <Composer
      tenderId="submitted-one"
      onSend={async () => {}}
      onImport={() => {}}
    />,
  );
  await user.type(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
    "Review this scope",
  );
  await user.click(screen.getByRole("button", { name: "Send instruction" }));
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Instruction accepted",
  );
  view.unmount();
  render(
    <Composer
      tenderId="submitted-one"
      onSend={async () => {}}
      onImport={() => {}}
    />,
  );
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("");
});

it("retains the engineer instruction when starting a manager run fails", async () => {
  const user = userEvent.setup();
  render(
    <Composer
      onSend={async () => {
        throw new Error("Manager could not start.");
      }}
      onImport={() => {}}
    />,
  );
  const input = screen.getByRole("textbox", {
    name: "Message to Tender Manager",
  });
  await user.type(input, "Check the drainage scope");
  await user.click(screen.getByRole("button", { name: "Send instruction" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Manager could not start.",
  );
  expect(input).toHaveValue("Check the drainage scope");
});

it("sends while work is busy and reuses the idempotency key after a network failure", async () => {
  const user = userEvent.setup();
  const submissions: Array<[string, string]> = [];
  let failed = true;
  render(
    <Composer
      busy
      onSend={async (content, key) => {
        submissions.push([content, key]);
        if (failed) {
          failed = false;
          throw new Error("The local service could not be reached.");
        }
      }}
      onImport={() => {}}
    />,
  );
  const input = screen.getByRole("textbox", {
    name: "Message to Tender Manager",
  });
  await user.type(input, "Review the tender documents");
  await user.click(screen.getByRole("button", { name: "Send when ready" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("local service");
  await user.click(screen.getByRole("button", { name: "Send when ready" }));
  expect(submissions).toHaveLength(2);
  expect(submissions[0][1]).toBe(submissions[1][1]);
  expect(input).toHaveValue("");
});

it("reuses the retry identity after remount and gives changed text a fresh identity", async () => {
  const user = userEvent.setup();
  const submissions: Array<[string, string]> = [];
  const onSend = async (content: string, key: string) => {
    submissions.push([content, key]);
    throw new Error("Connection lost");
  };
  const props = { tenderId: "retry-remount", onSend, onImport: () => {} };
  let view = render(<Composer {...props} />);
  await user.type(screen.getByRole("textbox"), "Check drainage");
  await user.click(screen.getByRole("button", { name: "Send instruction" }));
  await screen.findByRole("alert");
  view.unmount();
  view = render(<Composer {...props} />);
  await user.click(screen.getByRole("button", { name: "Send instruction" }));
  await screen.findByRole("alert");
  expect(submissions[1]).toEqual(submissions[0]);
  await user.type(screen.getByRole("textbox"), " quantities");
  await user.click(screen.getByRole("button", { name: "Send instruction" }));
  expect(submissions[2][1]).not.toBe(submissions[0][1]);
});

it("clears the accepted raw whitespace snapshot and protects a later mounted draft", async () => {
  let resolve: (() => void) | undefined;
  const request = new Promise<void>((done) => {
    resolve = done;
  });
  const props = {
    tenderId: "in-flight-remount",
    onSend: async () => request,
    onImport: () => {},
  };
  let view = render(<Composer {...props} initialDraft="  Review scope  " />);
  fireEvent.click(screen.getByRole("button", { name: "Send instruction" }));
  view.unmount();
  view = render(<Composer {...props} />);
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "Later instruction" },
  });
  await act(async () => {
    resolve?.();
    await request;
  });
  view.unmount();
  view = render(<Composer {...props} />);
  expect(screen.getByRole("textbox")).toHaveValue("Later instruction");
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "  Review scope  " },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send instruction" }));
  await waitFor(() => expect(screen.getByRole("textbox")).toHaveValue(""));
  view.unmount();
  render(<Composer {...props} />);
  expect(screen.getByRole("textbox")).toHaveValue("");
});

it("keeps Tender drafts isolated when the component receives another Tender", () => {
  const props = { onSend: async () => {}, onImport: () => {} };
  const view = render(<Composer tenderId="tender-a" {...props} />);
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "Tender A scope" },
  });
  view.rerender(<Composer tenderId="tender-b" {...props} />);
  expect(screen.getByRole("textbox")).toHaveValue("");
  view.rerender(<Composer tenderId="tender-a" {...props} />);
  expect(screen.getByRole("textbox")).toHaveValue("Tender A scope");
});

it("keeps one waiting instruction and respects IME and multiline input", async () => {
  let sends = 0;
  const props = {
    onSend: async () => {
      sends += 1;
    },
    onImport: () => {},
  };
  const view = render(
    <Composer hasPending {...props} initialDraft="Review scope" />,
  );
  fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
  expect(sends).toBe(0);
  expect(
    screen.getByRole("button", { name: "Send instruction" }),
  ).toBeDisabled();
  view.rerender(<Composer {...props} initialDraft="Review scope" />);
  fireEvent.keyDown(screen.getByRole("textbox"), {
    key: "Enter",
    isComposing: true,
  });
  fireEvent.keyDown(screen.getByRole("textbox"), {
    key: "Enter",
    shiftKey: true,
  });
  expect(sends).toBe(0);
  fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
  fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
  await waitFor(() => expect(sends).toBe(1));
});

it("keeps newer text typed while an accepted submission is in flight", async () => {
  const user = userEvent.setup();
  let resolve: (() => void) | undefined;
  const sent = new Promise<void>((done) => {
    resolve = done;
  });
  render(<Composer onSend={async () => sent} onImport={() => {}} />);
  const input = screen.getByRole("textbox", {
    name: "Message to Tender Manager",
  });
  await user.type(input, "First instruction");
  await user.click(screen.getByRole("button", { name: "Send instruction" }));
  await user.type(input, "Second instruction");
  resolve?.();
  await screen.findByRole("status");
  expect(input).toHaveValue("First instructionSecond instruction");
});
