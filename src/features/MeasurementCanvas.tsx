import { useState } from "react";
import type { Schema } from "../api";

export type MeasurementPoint = [number, number];
export function MeasurementCanvas({
  source,
  url,
  points,
  calibration,
  mode,
  onPoint,
  disabled = false,
}: {
  source: Schema<"MeasurementPage">;
  url: string;
  points: MeasurementPoint[];
  calibration: MeasurementPoint[];
  mode: Schema<"MeasurementInput">["mode"];
  onPoint?: (point: MeasurementPoint) => void;
  disabled?: boolean;
}) {
  const [loaded, setLoaded] = useState(false);
  const [width, height] = source.page_size;
  const coordinates = (values: MeasurementPoint[]) =>
    values.map(([x, y]) => `${x * width},${y * height}`).join(" ");
  const radius = Math.min(width, height) / 110;
  return (
    <div className="measurement-canvas">
      <img
        src={url}
        alt={`${source.name}, page ${source.page}`}
        onLoad={() => setLoaded(true)}
        draggable={false}
      />
      <svg
        aria-label="Measurement overlay"
        role="img"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        className={onPoint && !disabled && loaded ? "measurement-markable" : ""}
        onClick={(event) => {
          if (!onPoint || disabled || !loaded) return;
          const bounds = event.currentTarget.getBoundingClientRect();
          if (!bounds.width || !bounds.height) return;
          const x = (event.clientX - bounds.left) / bounds.width,
            y = (event.clientY - bounds.top) / bounds.height;
          if (x >= 0 && x <= 1 && y >= 0 && y <= 1) onPoint([x, y]);
        }}
      >
        {mode === "area" && points.length >= 3 ? (
          <polygon
            points={coordinates(points)}
            className="measurement-area"
            vectorEffect="non-scaling-stroke"
          />
        ) : null}
        {mode === "length" && points.length >= 2 ? (
          <polyline
            points={coordinates(points)}
            className="measurement-line"
            vectorEffect="non-scaling-stroke"
          />
        ) : null}
        {calibration.length === 2 ? (
          <polyline
            points={coordinates(calibration)}
            className="measurement-calibration"
            vectorEffect="non-scaling-stroke"
          />
        ) : null}
        {points.map(([x, y], index) => (
          <circle
            key={index}
            cx={x * width}
            cy={y * height}
            r={radius}
            className="measurement-point"
            vectorEffect="non-scaling-stroke"
          >
            <title>Mark {index + 1}</title>
          </circle>
        ))}
        {calibration.map(([x, y], index) => (
          <circle
            key={index}
            cx={x * width}
            cy={y * height}
            r={radius}
            className="measurement-calibration-point"
            vectorEffect="non-scaling-stroke"
          >
            <title>Calibration point {index + 1}</title>
          </circle>
        ))}
      </svg>
    </div>
  );
}
