import {
  Button,
  Menu,
  MenuItem,
  MenuTrigger,
  Popover,
} from "react-aria-components";
import type { MenuTriggerProps } from "react-aria-components";
import type { ReactNode } from "react";

export interface QuantixMenuItem {
  id: string;
  label: string;
  icon?: ReactNode;
  disabled?: boolean;
  /** Explains why a disabled action is unavailable. */
  description?: string;
}

export interface QuantixMenuProps {
  label: string;
  items: readonly QuantixMenuItem[];
  onAction: (id: string) => void;
  children?: ReactNode;
  /** Allows row context-menu and keyboard invocation to control this menu. */
  isOpen?: MenuTriggerProps["isOpen"];
  onOpenChange?: MenuTriggerProps["onOpenChange"];
  trigger?: MenuTriggerProps["trigger"];
}

export function QuantixMenu({
  label,
  items,
  onAction,
  children,
  isOpen,
  onOpenChange,
  trigger,
}: QuantixMenuProps) {
  return (
    <MenuTrigger
      {...(isOpen === undefined ? {} : { isOpen })}
      {...(onOpenChange === undefined ? {} : { onOpenChange })}
      {...(trigger === undefined ? {} : { trigger })}
    >
      <Button className="qx-ui-menu__trigger" aria-label={label}>
        {children ?? "More"}
      </Button>
      <Popover className="qx-ui-popover qx-ui-menu__popover">
        <Menu className="qx-ui-menu" onAction={(key) => onAction(String(key))}>
          {items.map((item) => (
            <MenuItem
              key={item.id}
              id={item.id}
              isDisabled={item.disabled}
              className="qx-ui-menu__item"
            >
              {item.icon}
              <span className="qx-ui-menu__item-copy">
                <span>{item.label}</span>
                {item.description && <small>{item.description}</small>}
              </span>
            </MenuItem>
          ))}
        </Menu>
      </Popover>
    </MenuTrigger>
  );
}
