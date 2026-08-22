import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type WindowTitleBarMenu, WindowTitleBar } from "./WindowTitleBar";

// @ts-expect-error process is a Node.js global shared by the Vitest pool.
const appWindow = process.__QUANTIX_TEST_APP_WINDOW__ as {
  close: ReturnType<typeof vi.fn>;
  isMaximized: ReturnType<typeof vi.fn>;
  minimize: ReturnType<typeof vi.fn>;
  onResized: ReturnType<typeof vi.fn>;
  toggleMaximize: ReturnType<typeof vi.fn>;
};

const fileAction = vi.fn();
const menus: readonly WindowTitleBarMenu[] = [
  {
    id: "file",
    label: "File",
    items: [
      {
        id: "new",
        label: "New Tender",
        shortcut: "Ctrl+N",
        onSelect: fileAction,
      },
    ],
  },
  {
    id: "edit",
    label: "Edit",
    items: [
      { id: "undo", label: "Undo", editCommand: "undo" },
      { id: "edit-separator", type: "separator" },
      { id: "copy", label: "Copy", editCommand: "copy" },
    ],
  },
  { id: "view", label: "View", items: [] },
  { id: "help", label: "Help", items: [] },
];

describe("WindowTitleBar", () => {
  const execCommand = vi.fn();

  beforeEach(() => {
    appWindow.close.mockResolvedValue(undefined);
    appWindow.isMaximized.mockResolvedValue(false);
    appWindow.minimize.mockResolvedValue(undefined);
    appWindow.onResized.mockResolvedValue(vi.fn());
    appWindow.toggleMaximize.mockResolvedValue(undefined);
    Object.defineProperty(document, "execCommand", {
      configurable: true,
      value: execCommand,
    });
  });

  afterEach(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    delete document.documentElement.dataset.quantixRuntime;
    cleanup();
    vi.clearAllMocks();
  });

  it("renders only when enabled and exposes a direct drag region", () => {
    const { container, rerender } = render(
      <WindowTitleBar enabled={false} menus={menus} />,
    );
    expect(container.firstChild).toBeNull();

    rerender(<WindowTitleBar enabled menus={menus} />);
    expect(screen.getByRole("banner")).toBeTruthy();
    expect(
      screen.getAllByRole("menuitem").map((item) => item.textContent),
    ).toEqual(["File", "Edit", "View", "Help"]);
    expect(
      container
        .querySelector(".window-title-bar__drag-region")
        ?.hasAttribute("data-tauri-drag-region"),
    ).toBe(true);
  });

  it("honors navigation capabilities and callbacks", () => {
    const onToggleSidebar = vi.fn();
    const onBack = vi.fn();
    const onForward = vi.fn();
    const { rerender } = render(
      <WindowTitleBar
        enabled
        menus={menus}
        onToggleSidebar={onToggleSidebar}
        onBack={onBack}
        onForward={onForward}
        canGoBack={false}
        canGoForward
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Hide Tenders" }));
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    fireEvent.click(screen.getByRole("button", { name: "Forward" }));

    expect(onToggleSidebar).toHaveBeenCalledTimes(1);
    expect(onBack).not.toHaveBeenCalled();
    expect(onForward).toHaveBeenCalledTimes(1);

    rerender(
      <WindowTitleBar
        enabled
        menus={menus}
        sidebarVisible={false}
        canToggleSidebar={false}
      />,
    );
    expect(screen.getByRole("button", { name: "Show Tenders" })).toHaveProperty(
      "disabled",
      true,
    );
  });

  it("runs menu callbacks and edit commands, then closes the menu", () => {
    render(
      <div>
        <WindowTitleBar enabled menus={menus} />
        <input aria-label="Tender name" />
      </div>,
    );

    fireEvent.click(screen.getByRole("menuitem", { name: "File" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /New Tender/ }));
    expect(fileAction).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu", { name: "File" })).toBeNull();

    fireEvent.click(screen.getByRole("menuitem", { name: "Edit" }));
    const copy = screen.getByRole("menuitem", { name: "Copy" });
    expect(copy).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit" }));

    const input = screen.getByRole("textbox", { name: "Tender name" });
    input.focus();
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy" }));
    expect(execCommand).toHaveBeenCalledWith("copy");
    expect(input).toBe(document.activeElement);
    expect(screen.queryByRole("menu", { name: "Edit" })).toBeNull();
  });

  it("uses the last focused editable target, including contenteditable", () => {
    execCommand.mockImplementation(() => {
      expect(screen.getByTestId("editable-copy")).toBe(document.activeElement);
      return true;
    });
    render(
      <div>
        <WindowTitleBar enabled menus={menus} />
        <textarea aria-label="First editor" />
        <div
          contentEditable
          role="textbox"
          aria-label="Latest editor"
          data-testid="editable-copy"
        />
      </div>,
    );

    screen.getByRole("textbox", { name: "First editor" }).focus();
    screen.getByRole("textbox", { name: "Latest editor" }).focus();
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy" }));

    expect(execCommand).toHaveBeenCalledWith("copy");
  });

  it("supports menu keyboard navigation, Escape, and outside clicks", async () => {
    render(
      <div>
        <WindowTitleBar enabled menus={menus} />
        <input aria-label="Keyboard edit target" />
        <button type="button">Outside</button>
      </div>,
    );

    screen.getByRole("textbox", { name: "Keyboard edit target" }).focus();
    const fileMenu = screen.getByRole("menuitem", { name: "File" });
    fileMenu.focus();
    fireEvent.keyDown(fileMenu, { key: "ArrowDown" });
    await waitFor(() =>
      expect(screen.getByRole("menuitem", { name: /New Tender/ })).toBe(
        document.activeElement,
      ),
    );

    fireEvent.keyDown(document.activeElement as Element, { key: "ArrowRight" });
    await waitFor(() =>
      expect(screen.getByRole("menuitem", { name: "Undo" })).toBe(
        document.activeElement,
      ),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu", { name: "Edit" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Edit" })).toBe(
      document.activeElement,
    );

    fireEvent.click(fileMenu);
    expect(screen.getByRole("menu", { name: "File" })).toBeTruthy();
    fireEvent.mouseDown(screen.getByRole("button", { name: "Outside" }));
    expect(screen.queryByRole("menu", { name: "File" })).toBeNull();
  });

  it("guards native controls and reconciles maximize state on native resize", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    render(<WindowTitleBar enabled menus={menus} />);

    await waitFor(() => {
      expect(appWindow.isMaximized).toHaveBeenCalledTimes(1);
      expect(appWindow.onResized).toHaveBeenCalledTimes(1);
    });
    fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
    fireEvent.click(screen.getByRole("button", { name: "Maximize" }));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));

    await waitFor(() => {
      expect(appWindow.minimize).toHaveBeenCalledTimes(1);
      expect(appWindow.toggleMaximize).toHaveBeenCalledTimes(1);
      expect(appWindow.close).toHaveBeenCalledTimes(1);
      expect(screen.getByRole("button", { name: "Restore" })).toBeTruthy();
    });

    const onResized = appWindow.onResized.mock.calls[0]?.[0] as () => void;
    await act(async () => {
      onResized();
    });
    await waitFor(() => {
      expect(appWindow.isMaximized).toHaveBeenCalledTimes(2);
      expect(screen.getByRole("button", { name: "Maximize" })).toBeTruthy();
    });
  });

  it("does not call native APIs in a browser preview", () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    document.documentElement.dataset.quantixRuntime = "browser-preview";
    render(<WindowTitleBar enabled menus={menus} />);
    fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
    fireEvent.click(screen.getByRole("button", { name: "Maximize" }));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(appWindow.minimize).not.toHaveBeenCalled();
    expect(appWindow.toggleMaximize).not.toHaveBeenCalled();
    expect(appWindow.close).not.toHaveBeenCalled();
    expect(appWindow.onResized).not.toHaveBeenCalled();
  });
});
