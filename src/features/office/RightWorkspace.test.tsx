import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RightWorkspace } from "./RightWorkspace";
import { useWorkspaceState } from "./workspace-state";
import { beforeEach } from "vitest";

// The side pane now starts collapsed. These tests are about what it does once
// it is open, so they restore the saved "open" preference the app honours.
beforeEach(() => {
  localStorage.setItem("quantix.right-workspace.v2", "split");
});

function TestWorkspace() {
  const workspace = useWorkspaceState();
  return (
    <RightWorkspace
      workspace={workspace}
      title="Synthetic Tender"
      chat={
        <label>
          Manager draft
          <input defaultValue="Keep my instruction" />
        </label>
      }
      context={<p>Synthetic sources</p>}
      renderView={(view) => (
        <label>
          {view} draft
          <input defaultValue="Keep work" />
        </label>
      )}
    />
  );
}

it("opens the team alongside documents and retains its draft when switching tabs", async () => {
  const user = userEvent.setup();
  render(<TestWorkspace />);
  await user.click(screen.getByRole("button", { name: /^Team/ }));
  const draft = screen.getByRole("textbox", { name: "team draft" });
  await user.type(draft, " retained");
  await user.click(screen.getByRole("button", { name: "Workspace home" }));
  await user.click(screen.getByRole("button", { name: /Documents/ }));
  await user.click(screen.getByRole("tab", { name: /Team/ }));
  expect(screen.getByRole("textbox", { name: "team draft" })).toBe(draft);
  expect(draft).toHaveValue("Keep work retained");
});

it("collapses and expands the full workspace without remounting the Manager draft", async () => {
  const user = userEvent.setup();
  render(<TestWorkspace />);
  const draft = screen.getByRole("textbox", { name: "Manager draft" });
  await user.type(draft, " edited");
  await user.click(screen.getByRole("button", { name: "Hide workspace" }));
  expect(
    screen.queryByRole("complementary", { name: "Tender workspace" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Show workspace" }));
  await user.click(screen.getByRole("button", { name: "Expand workspace" }));
  expect(draft).not.toBeVisible();
  await user.click(screen.getByRole("button", { name: "Restore split view" }));
  expect(screen.getByRole("textbox", { name: "Manager draft" })).toBe(draft);
  expect(draft).toHaveValue("Keep my instruction edited");
});

it("keeps visited tab state, closes a tab, and returns to the launcher", async () => {
  const user = userEvent.setup();
  render(<TestWorkspace />);
  await user.click(screen.getByRole("button", { name: /^Documents/ }));
  const draft = screen.getByRole("textbox", { name: "documents draft" });
  await user.type(draft, " edited");
  await user.click(screen.getByRole("button", { name: "Workspace home" }));
  await user.click(screen.getByRole("button", { name: /^Team/ }));
  await user.click(screen.getByRole("tab", { name: "Documents" }));
  expect(screen.getByRole("textbox", { name: "documents draft" })).toBe(draft);
  expect(draft).toHaveValue("Keep work edited");
  await user.click(screen.getByRole("button", { name: "Close Documents tab" }));
  expect(
    screen.queryByRole("tab", { name: "Documents" }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Close Team tab" }));
  expect(
    screen.getByRole("button", { name: /^Documents/ }),
  ).toBeInTheDocument();
});

it("opens context in an anchored popover and keeps keyboard shortcuts out of text inputs", async () => {
  const user = userEvent.setup();
  render(<TestWorkspace />);
  await user.click(screen.getByRole("button", { name: "Tender context" }));
  expect(
    within(screen.getByRole("dialog")).getByText("Synthetic sources"),
  ).toBeInTheDocument();
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("textbox", { name: "Manager draft" }));
  await user.keyboard("{Control>}{Alt>}w{/Alt}{/Control}");
  expect(
    screen.getByRole("button", { name: "Hide workspace" }),
  ).toBeInTheDocument();
});

it("bounds restored split widths and supports keyboard resizing", async () => {
  const user = userEvent.setup();
  localStorage.setItem("quantix.office-split.v2", "99");
  render(<TestWorkspace />);
  const divider = screen.getByRole("separator", {
    name: "Resize conversation and workspace",
  });
  expect(divider).toHaveAttribute("aria-valuenow", "74");
  divider.focus();
  await user.keyboard("{Home}");
  expect(divider).toHaveAttribute("aria-valuenow", "32");
  await user.keyboard("{ArrowRight}");
  expect(divider).toHaveAttribute("aria-valuenow", "34");
});

it("starts collapsed so the conversation owns the window until the pane is opened", async () => {
  const user = userEvent.setup();
  localStorage.removeItem("quantix.right-workspace.v2");
  render(<TestWorkspace />);
  expect(
    screen.queryByRole("button", { name: "Hide workspace" }),
  ).not.toBeInTheDocument();
  const show = screen.getByRole("button", { name: "Show workspace" });
  await user.click(show);
  expect(
    screen.getByRole("button", { name: "Hide workspace" }),
  ).toBeInTheDocument();
  // Opening it on purpose is remembered for the next visit.
  expect(localStorage.getItem("quantix.right-workspace.v2")).toBe("split");
});
