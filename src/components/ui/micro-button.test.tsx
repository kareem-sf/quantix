import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MicroButton } from "./micro-button";
import { ThemeProvider } from "@/theme";
import { ThemeToggleButton } from "./theme-toggle";

it("keeps a destructive action inert on hover and focus and activates on Enter", async () => {
  const user = userEvent.setup();
  const action = vi.fn();
  const submit = vi.fn((event: React.FormEvent) => event.preventDefault());
  render(
    <form onSubmit={submit}>
      <MicroButton kind="delete" onClick={action}>
        Remove account
      </MicroButton>
    </form>,
  );
  const button = screen.getByRole("button", { name: "Remove account" });
  await user.hover(button);
  await user.tab();
  expect(action).not.toHaveBeenCalled();
  await user.keyboard("{Enter}");
  expect(action).toHaveBeenCalledTimes(1);
  expect(submit).not.toHaveBeenCalled();
});

it("preserves disabled confirmation and explicit submit behavior", async () => {
  const user = userEvent.setup();
  const submit = vi.fn((event: React.FormEvent) => event.preventDefault());
  const tree = (disabled: boolean) => (
    <form onSubmit={submit}>
      <MicroButton kind="delete" type="submit" disabled={disabled}>
        Delete
      </MicroButton>
    </form>
  );
  const view = render(tree(true));
  await user.click(screen.getByRole("button", { name: "Delete" }));
  expect(submit).not.toHaveBeenCalled();
  view.rerender(tree(false));
  await user.click(screen.getByRole("button", { name: "Delete" }));
  expect(submit).toHaveBeenCalledTimes(1);
});

it("changes preview and search icons only with the controlled action state", async () => {
  const user = userEvent.setup();
  const view = render(
    <MicroButton kind="preview" active={false}>
      Preview
    </MicroButton>,
  );
  const button = screen.getByRole("button", { name: "Preview" });
  await user.hover(button);
  expect(button.querySelector(".lucide-play")).toBeInTheDocument();
  expect(button.querySelector(".lucide-pause")).not.toBeInTheDocument();
  view.rerender(
    <MicroButton kind="preview" active>
      Hide preview
    </MicroButton>,
  );
  expect(
    screen.getByRole("button").querySelector(".lucide-pause"),
  ).toBeInTheDocument();
  view.rerender(
    <MicroButton kind="search" active>
      Clear search
    </MicroButton>,
  );
  expect(
    screen.getByRole("button").querySelector(".lucide-x"),
  ).toBeInTheDocument();
});

it("switches the real theme by keyboard and stores the selected preference", async () => {
  const user = userEvent.setup();
  render(
    <ThemeProvider>
      <ThemeToggleButton />
    </ThemeProvider>,
  );
  await user.tab();
  await user.keyboard("{Enter}");
  expect(document.documentElement).toHaveClass("dark");
  expect(window.localStorage.getItem("quantix-theme")).toBe("dark");
  await user.click(
    screen.getByRole("button", { name: "Switch to light theme" }),
  );
  expect(document.documentElement).not.toHaveClass("dark");
});
