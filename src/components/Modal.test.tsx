import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ErrorNotice, Modal } from "./common";
import { ApiError } from "../api";

it("focuses the first field, traps Tab and restores the opener after Escape", async () => {
  const user = userEvent.setup();
  function Example() {
    const [open, setOpen] = useState(false);
    return (
      <>
        <button onClick={() => setOpen(true)}>Open review</button>
        {open ? (
          <Modal title="Review" onClose={() => setOpen(false)}>
            <label>
              Note
              <textarea />
            </label>
            <details>
              <summary>More options</summary>
              <button>Hidden action</button>
            </details>
            <button>Last action</button>
          </Modal>
        ) : null}
      </>
    );
  }
  render(<Example />);
  const opener = screen.getByRole("button", { name: "Open review" });
  await user.click(opener);
  await waitFor(() =>
    expect(screen.getByRole("textbox", { name: "Note" })).toHaveFocus(),
  );
  screen.getByRole("button", { name: "Last action" }).focus();
  await user.tab();
  expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();
  await user.tab({ shift: true });
  expect(screen.getByRole("button", { name: "Last action" })).toHaveFocus();
  await user.keyboard("{Escape}");
  await waitFor(() =>
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
  );
  await waitFor(() => expect(opener).toHaveFocus());
});

it("keeps error recovery visible and the request reference in More options", async () => {
  const user = userEvent.setup();
  render(
    <ErrorNotice
      error={
        new ApiError(
          "Check the selected model before continuing.",
          409,
          "a".repeat(32),
        )
      }
    />,
  );
  expect(
    screen.getByText("Check the selected model before continuing."),
  ).toBeVisible();
  expect(screen.getByText("a".repeat(32))).not.toBeVisible();
  await user.click(screen.getByText("More options"));
  expect(screen.getByText("a".repeat(32))).toBeVisible();
});
