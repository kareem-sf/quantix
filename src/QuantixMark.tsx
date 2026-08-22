import markColor from "./assets/brand/quantix-mark.svg";
import markInverse from "./assets/brand/quantix-mark-inverse.svg";
import markMono from "./assets/brand/quantix-mark-mono.svg";
import "./QuantixMark.css";

type QuantixMarkProps = {
  className?: string;
  label?: string;
  variant?: "auto" | "color" | "inverse" | "mono";
};

const sources = {
  color: markColor,
  inverse: markInverse,
  mono: markMono,
};

export function QuantixMark({
  className,
  label,
  variant = "auto",
}: QuantixMarkProps) {
  if (variant === "auto") {
    return (
      <span
        className={["quantix-mark", className].filter(Boolean).join(" ")}
        role={label ? "img" : undefined}
        aria-label={label}
        aria-hidden={label ? undefined : true}
      >
        <img className="quantix-mark__color" src={markColor} alt="" />
        <img className="quantix-mark__inverse" src={markInverse} alt="" />
      </span>
    );
  }

  return (
    <span
      className={["quantix-mark", className].filter(Boolean).join(" ")}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      <img className="quantix-mark__single" src={sources[variant]} alt="" />
    </span>
  );
}
