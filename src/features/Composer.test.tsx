import { render, screen } from "@testing-library/react";
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
    "Instruction submitted",
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

it("keeps a submitted draft recoverable until the run finishes", async () => {
  const user = userEvent.setup();
  render(<Composer onSend={async () => {}} onImport={() => {}} />);
  const input = screen.getByRole("textbox", {
    name: "Message to Tender Manager",
  });
  await user.type(input, "Review the tender documents");
  await user.click(screen.getByRole("button", { name: "Send instruction" }));
  expect(input).toHaveValue("Review the tender documents");
  expect(screen.getByRole("status")).toHaveTextContent("Instruction submitted");
});
