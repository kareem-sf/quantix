import {
  Button,
  Label,
  ListBox,
  ListBoxItem,
  Popover,
  Select,
  SelectValue,
} from "react-aria-components";
import { ChevronDown } from "lucide-react";

export interface QuantixSelectOption {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
}

interface QuantixSelectProps {
  "aria-label"?: string;
  label: string;
  description?: string;
  value: string;
  options: readonly QuantixSelectOption[];
  disabled?: boolean;
  onChange: (value: string) => void;
}

export function QuantixSelect({
  label,
  description,
  value,
  options,
  disabled = false,
  onChange,
  "aria-label": ariaLabel,
}: QuantixSelectProps) {
  const selected = options.find((option) => option.value === value);

  return (
    <Select
      className="qx-ui-select"
      aria-label={ariaLabel}
      selectedKey={value || null}
      isDisabled={disabled}
      onSelectionChange={(key) => {
        if (key != null) onChange(String(key));
      }}
    >
      <Label>{label}</Label>
      <Button className="qx-ui-select__trigger">
        <SelectValue>{selected?.label ?? "Choose an option"}</SelectValue>
        <ChevronDown size={16} aria-hidden="true" />
      </Button>
      {description ? (
        <span className="qx-ui-select__description">{description}</span>
      ) : null}
      <Popover className="qx-ui-popover qx-ui-select__popover">
        <ListBox className="qx-ui-listbox">
          {options.map((option) => (
            <ListBoxItem
              key={option.value}
              id={option.value}
              textValue={option.label}
              isDisabled={option.disabled}
              className="qx-ui-listbox__item"
            >
              <span>{option.label}</span>
              {option.description ? <small>{option.description}</small> : null}
            </ListBoxItem>
          ))}
        </ListBox>
      </Popover>
    </Select>
  );
}
