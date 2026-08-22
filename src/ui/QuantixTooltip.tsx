import { Button, Tooltip, TooltipTrigger } from "react-aria-components";
import type { ReactNode } from "react";

interface QuantixTooltipProps {
  label: string;
  children: ReactNode;
}

export function QuantixTooltip({ label, children }: QuantixTooltipProps) {
  return (
    <TooltipTrigger delay={350}>
      <Button className="qx-ui-tooltip__trigger" aria-label={label}>
        {children}
      </Button>
      <Tooltip className="qx-ui-tooltip">{label}</Tooltip>
    </TooltipTrigger>
  );
}
