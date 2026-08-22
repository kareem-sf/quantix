import { Switch } from "react-aria-components";

interface QuantixSwitchProps {
  checked: boolean;
  disabled?: boolean;
  label: string;
  summary: string;
  onChange: (checked: boolean) => void;
}

export function QuantixSwitch({
  checked,
  disabled = false,
  label,
  summary,
  onChange,
}: QuantixSwitchProps) {
  return (
    <Switch
      className="qx-ui-switch"
      isSelected={checked}
      isDisabled={disabled}
      onChange={onChange}
    >
      <span className="qx-ui-switch__copy">
        <strong>{label}</strong>
        <small>{summary}</small>
      </span>
      <span className="qx-ui-switch__control" aria-hidden="true">
        <span />
      </span>
    </Switch>
  );
}
