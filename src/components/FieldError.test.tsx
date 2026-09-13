import { render, screen } from "@testing-library/react";
import { ApiError } from "../api";
import { FieldError, findFieldError } from "./FieldError";
it("places only the matching nested field error beside its control", () => {
  const error = new ApiError(
    "Review the highlighted fields.",
    422,
    "a".repeat(32),
    [
      {
        path: ["body", "programme", "activities", 0, "duration_days"],
        message: "Use at least one working day.",
      },
      {
        path: ["body", "programme", "activities", 1, "duration_days"],
        message: "Use at most 10000 working days.",
      },
    ],
  );
  render(
    <label>
      First duration
      <input />
      <FieldError
        error={error}
        path={["programme", "activities", 0, "duration_days"]}
      />
    </label>,
  );
  expect(screen.getByRole("alert")).toHaveTextContent(
    "Use at least one working day.",
  );
  expect(
    screen.queryByText("Use at most 10000 working days."),
  ).not.toBeInTheDocument();
  expect(findFieldError(error, ["missing"])).toBeUndefined();
  expect(screen.queryByText(/Request reference/)).not.toBeInTheDocument();
});
