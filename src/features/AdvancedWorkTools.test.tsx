import { nativeSelectionErrors } from "./AdvancedWorkTools";

it("requires a complete hosted code allowance and reviewed page domains", () => {
  expect(
    nativeSelectionErrors(
      { route: { native_tools: ["code_execution"], max_calls_per_request: 3 } },
      [],
    ),
  ).toContain(
    "Record the provider's documented code session price in AI accounts.",
  );
  expect(
    nativeSelectionErrors(
      { route: { native_tools: ["web_fetch"], max_calls_per_request: 3 } },
      [],
    ),
  ).toContain(
    "Enter the public domains that provider page reading may access.",
  );
});
