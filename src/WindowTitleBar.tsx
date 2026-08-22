import {
  ArrowLeft,
  ArrowRight,
  Minus,
  PanelLeft,
  Square,
  X,
} from "lucide-react";
import {
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import "./WindowTitleBar.css";

export type WindowTitleBarEditCommand =
  "undo" | "redo" | "cut" | "copy" | "paste" | "selectAll";

export interface WindowTitleBarMenuCommand {
  id: string;
  type?: "command";
  label: string;
  shortcut?: string;
  disabled?: boolean;
  editCommand?: WindowTitleBarEditCommand;
  onSelect?: () => void;
}

export interface WindowTitleBarMenuSeparator {
  id: string;
  type: "separator";
}

export type WindowTitleBarMenuItem =
  WindowTitleBarMenuCommand | WindowTitleBarMenuSeparator;

export interface WindowTitleBarMenu {
  id: string;
  label: string;
  items: readonly WindowTitleBarMenuItem[];
}

export interface WindowTitleBarProps {
  enabled?: boolean;
  sidebarVisible?: boolean;
  canToggleSidebar?: boolean;
  onToggleSidebar?: () => void;
  canGoBack?: boolean;
  onBack?: () => void;
  canGoForward?: boolean;
  onForward?: () => void;
  menus: readonly WindowTitleBarMenu[];
}

function windowsTitleBarEnabledByDefault() {
  return (
    typeof __QUANTIX_WINDOWS_TITLEBAR__ !== "undefined" &&
    __QUANTIX_WINDOWS_TITLEBAR__
  );
}

function runningInTauri() {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in window &&
    document.documentElement.dataset.quantixRuntime !== "browser-preview"
  );
}

function swallowWindowControlError(action: () => Promise<unknown>) {
  if (!runningInTauri()) return;
  void action().catch(() => undefined);
}

function menuCommands(menu: HTMLElement | null) {
  if (!menu) return [];
  return Array.from(
    menu.querySelectorAll<HTMLButtonElement>(
      '[role="menuitem"]:not(:disabled)',
    ),
  );
}

const NON_TEXT_INPUT_TYPES = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);

function editableTargetFromNode(node: EventTarget | null) {
  if (!(node instanceof HTMLElement)) return null;

  if (node instanceof HTMLTextAreaElement) {
    return node.disabled || node.readOnly ? null : node;
  }
  if (node instanceof HTMLInputElement) {
    return node.disabled || node.readOnly || NON_TEXT_INPUT_TYPES.has(node.type)
      ? null
      : node;
  }

  const contentEditable = node.closest<HTMLElement>("[contenteditable]");
  if (
    contentEditable &&
    contentEditable.getAttribute("contenteditable")?.toLowerCase() !== "false"
  ) {
    return contentEditable;
  }
  return null;
}

function isAvailableEditableTarget(
  target: HTMLElement | null,
): target is HTMLElement {
  return (
    target !== null &&
    target.isConnected &&
    editableTargetFromNode(target) === target
  );
}

export function WindowTitleBar({
  enabled = windowsTitleBarEnabledByDefault(),
  sidebarVisible = true,
  canToggleSidebar = true,
  onToggleSidebar,
  canGoBack = false,
  onBack,
  canGoForward = false,
  onForward,
  menus,
}: WindowTitleBarProps) {
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const [maximized, setMaximized] = useState(false);
  const [hasEditableTarget, setHasEditableTarget] = useState(false);
  const titleBarRef = useRef<HTMLElement>(null);
  const lastEditableTargetRef = useRef<HTMLElement | null>(null);
  const triggerRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const menuRefs = useRef<Array<HTMLDivElement | null>>([]);
  const instanceId = useId();

  useEffect(() => {
    if (!enabled || !runningInTauri()) return;

    let active = true;
    let unlisten: (() => void) | undefined;
    const appWindow = getCurrentWindow();
    const updateMaximized = () => {
      void appWindow
        .isMaximized()
        .then((value) => {
          if (active) setMaximized(value);
        })
        .catch(() => undefined);
    };

    updateMaximized();
    void appWindow
      .onResized(() => {
        window.setTimeout(updateMaximized, 0);
      })
      .then((stopListening) => {
        if (active) unlisten = stopListening;
        else stopListening();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [enabled]);

  useEffect(() => {
    if (!enabled) return;

    const rememberEditableTarget = (event: FocusEvent) => {
      const editableTarget = editableTargetFromNode(event.target);
      if (!editableTarget) return;
      lastEditableTargetRef.current = editableTarget;
      setHasEditableTarget(true);
    };

    const initialTarget = editableTargetFromNode(document.activeElement);
    if (initialTarget) {
      lastEditableTargetRef.current = initialTarget;
      setHasEditableTarget(true);
    }

    document.addEventListener("focusin", rememberEditableTarget);
    return () =>
      document.removeEventListener("focusin", rememberEditableTarget);
  }, [enabled]);

  useEffect(() => {
    if (openMenuId === null) return;

    const closeOnOutsidePress = (event: MouseEvent) => {
      if (!titleBarRef.current?.contains(event.target as Node)) {
        setOpenMenuId(null);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      const openIndex = menus.findIndex((menu) => menu.id === openMenuId);
      setOpenMenuId(null);
      triggerRefs.current[openIndex]?.focus();
    };

    document.addEventListener("mousedown", closeOnOutsidePress);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOnOutsidePress);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [menus, openMenuId]);

  useEffect(() => {
    if (openMenuId === null) return;
    const openIndex = menus.findIndex((menu) => menu.id === openMenuId);
    menuCommands(menuRefs.current[openIndex] ?? null)[0]?.focus();
  }, [menus, openMenuId]);

  if (!enabled) return null;

  const openMenu = (index: number) => setOpenMenuId(menus[index]?.id ?? null);

  const moveBetweenMenus = (index: number, offset: number, expand: boolean) => {
    if (menus.length === 0) return;
    const nextIndex = (index + offset + menus.length) % menus.length;
    triggerRefs.current[nextIndex]?.focus();
    if (expand) openMenu(nextIndex);
  };

  const handleMenuTriggerKeyDown = (
    event: ReactKeyboardEvent<HTMLButtonElement>,
    index: number,
  ) => {
    switch (event.key) {
      case "ArrowDown":
      case "Enter":
      case " ":
        event.preventDefault();
        openMenu(index);
        break;
      case "ArrowRight":
        event.preventDefault();
        moveBetweenMenus(index, 1, openMenuId !== null);
        break;
      case "ArrowLeft":
        event.preventDefault();
        moveBetweenMenus(index, -1, openMenuId !== null);
        break;
      case "Home":
        event.preventDefault();
        triggerRefs.current[0]?.focus();
        break;
      case "End":
        event.preventDefault();
        triggerRefs.current[menus.length - 1]?.focus();
        break;
    }
  };

  const handleMenuItemKeyDown = (
    event: ReactKeyboardEvent<HTMLButtonElement>,
    menuIndex: number,
  ) => {
    const commands = menuCommands(menuRefs.current[menuIndex] ?? null);
    const commandIndex = commands.indexOf(event.currentTarget);

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        commands[(commandIndex + 1) % commands.length]?.focus();
        break;
      case "ArrowUp":
        event.preventDefault();
        commands[
          (commandIndex - 1 + commands.length) % commands.length
        ]?.focus();
        break;
      case "Home":
        event.preventDefault();
        commands[0]?.focus();
        break;
      case "End":
        event.preventDefault();
        commands[commands.length - 1]?.focus();
        break;
      case "ArrowRight":
        event.preventDefault();
        moveBetweenMenus(menuIndex, 1, true);
        break;
      case "ArrowLeft":
        event.preventDefault();
        moveBetweenMenus(menuIndex, -1, true);
        break;
      case "Escape":
        event.preventDefault();
        setOpenMenuId(null);
        triggerRefs.current[menuIndex]?.focus();
        break;
      case "Tab":
        setOpenMenuId(null);
        break;
    }
  };

  const selectMenuItem = (
    item: WindowTitleBarMenuCommand,
    menuIndex: number,
  ) => {
    setOpenMenuId(null);
    if (item.editCommand) {
      const editableTarget = lastEditableTargetRef.current;
      if (!isAvailableEditableTarget(editableTarget)) {
        lastEditableTargetRef.current = null;
        setHasEditableTarget(false);
        triggerRefs.current[menuIndex]?.focus();
        return;
      }
      editableTarget.focus();
      try {
        document.execCommand(item.editCommand);
      } catch {
        // WebView edit availability depends on the currently focused control.
      }
    }
    item.onSelect?.();
    if (!item.editCommand) triggerRefs.current[menuIndex]?.focus();
  };

  const toggleMaximize = async () => {
    if (!runningInTauri()) return;
    const appWindow = getCurrentWindow();
    try {
      await appWindow.toggleMaximize();
      setMaximized((current) => !current);
    } catch {
      // Native window commands can fail while the application is closing.
    }
  };

  const stopDoubleClick = (event: ReactMouseEvent) => event.stopPropagation();

  return (
    <header className="window-title-bar" ref={titleBarRef}>
      <div className="window-title-bar__app-controls">
        <button
          className="window-title-bar__icon-button"
          type="button"
          aria-label={sidebarVisible ? "Hide Tenders" : "Show Tenders"}
          aria-expanded={sidebarVisible}
          disabled={!canToggleSidebar}
          onClick={onToggleSidebar}
          onDoubleClick={stopDoubleClick}
        >
          <PanelLeft size={16} strokeWidth={1.5} aria-hidden="true" />
        </button>
        <button
          className="window-title-bar__icon-button"
          type="button"
          aria-label="Back"
          disabled={!canGoBack}
          onClick={onBack}
          onDoubleClick={stopDoubleClick}
        >
          <ArrowLeft size={16} strokeWidth={1.5} aria-hidden="true" />
        </button>
        <button
          className="window-title-bar__icon-button"
          type="button"
          aria-label="Forward"
          disabled={!canGoForward}
          onClick={onForward}
          onDoubleClick={stopDoubleClick}
        >
          <ArrowRight size={16} strokeWidth={1.5} aria-hidden="true" />
        </button>
      </div>

      <nav className="window-title-bar__menus" aria-label="Application menu">
        <div role="menubar" aria-label="Application commands">
          {menus.map((menu, menuIndex) => {
            const expanded = openMenuId === menu.id;
            const triggerId = `${instanceId}-menu-trigger-${menuIndex}`;
            const menuId = `${instanceId}-menu-${menuIndex}`;
            return (
              <div className="window-title-bar__menu" role="none" key={menu.id}>
                <button
                  className="window-title-bar__menu-trigger"
                  ref={(element) => {
                    triggerRefs.current[menuIndex] = element;
                  }}
                  id={triggerId}
                  type="button"
                  role="menuitem"
                  aria-haspopup="menu"
                  aria-expanded={expanded}
                  aria-controls={menuId}
                  tabIndex={menuIndex === 0 ? 0 : -1}
                  onClick={() =>
                    setOpenMenuId((current) =>
                      current === menu.id ? null : menu.id,
                    )
                  }
                  onKeyDown={(event) =>
                    handleMenuTriggerKeyDown(event, menuIndex)
                  }
                  onMouseEnter={() => {
                    if (openMenuId !== null) openMenu(menuIndex);
                  }}
                >
                  {menu.label}
                </button>
                {expanded ? (
                  <div
                    className="window-title-bar__menu-popover"
                    ref={(element) => {
                      menuRefs.current[menuIndex] = element;
                    }}
                    id={menuId}
                    role="menu"
                    aria-labelledby={triggerId}
                  >
                    {menu.items.map((item) =>
                      item.type === "separator" ? (
                        <div
                          className="window-title-bar__menu-separator"
                          role="separator"
                          key={item.id}
                        />
                      ) : (
                        <button
                          className="window-title-bar__menu-item"
                          type="button"
                          role="menuitem"
                          disabled={
                            item.disabled ||
                            (item.editCommand !== undefined &&
                              !hasEditableTarget)
                          }
                          key={item.id}
                          onClick={() => selectMenuItem(item, menuIndex)}
                          onKeyDown={(event) =>
                            handleMenuItemKeyDown(event, menuIndex)
                          }
                        >
                          <span>{item.label}</span>
                          {item.shortcut ? <kbd>{item.shortcut}</kbd> : null}
                        </button>
                      ),
                    )}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </nav>

      <div
        className="window-title-bar__drag-region"
        data-tauri-drag-region
        aria-hidden="true"
      />

      <div className="window-title-bar__window-controls">
        <button
          className="window-title-bar__caption-button"
          type="button"
          aria-label="Minimize"
          onClick={() =>
            swallowWindowControlError(() => getCurrentWindow().minimize())
          }
        >
          <Minus size={15} strokeWidth={1.4} aria-hidden="true" />
        </button>
        <button
          className="window-title-bar__caption-button"
          type="button"
          aria-label={maximized ? "Restore" : "Maximize"}
          onClick={() => void toggleMaximize()}
        >
          {maximized ? (
            <span
              className="window-title-bar__restore-icon"
              aria-hidden="true"
            />
          ) : (
            <Square size={11} strokeWidth={1.3} aria-hidden="true" />
          )}
        </button>
        <button
          className="window-title-bar__caption-button window-title-bar__caption-button--close"
          type="button"
          aria-label="Close"
          onClick={() =>
            swallowWindowControlError(() => getCurrentWindow().close())
          }
        >
          <X size={16} strokeWidth={1.25} aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}
