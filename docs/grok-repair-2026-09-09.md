# Grok access repair: permission and round budget

## Reproduced cause

The current generic access check reached three provider rounds. Its local native event metadata records `search_tool` followed by a denied permission decision in every round, then cancellation. The account was signed in and the model identity was the already recognized Build identity. No check tool ran.

The pinned upstream `ToolInput` classification makes discovery a `Read(None)` operation. Quantix's blanket `Read` deny therefore prevents tool discovery. The actual `use_tool` operation checks the inner MCP target name, so the existing `MCPTool(quantix__*)` rule remains necessary.

## Narrow implementation

1. In the bridge/execution configuration only, replace bare `Read` in the deny list with `Read(**)`. Leave the account-only deny list unchanged. Keep the curated agent tool list, the headless `--tools search_tool,use_tool` restriction, disabled subagents, shell/write/edit/grep/network prohibitions, and scoped MCP permission. No global auto-approval.
2. Give Grok checks a maximum of five model rounds: one combined discovery, one check-value call, one structured submission, one final response, with room for the original client to discover the two schemas separately. Keep the output cap at 1024 per round and subscription-only preflight unchanged. Other clients retain their existing check limits.
3. Use the same five-round value in setup preview/meter, host limits and worker validation. Prefer a named constant in each process contract, with a comment identifying the mirrored boundary. Do not add a general runtime-configuration subsystem for one protocol rule.
4. For Grok's generic check instruction, explicitly ask to discover both Quantix tools together, call the check tool once, submit its unchanged value using the submit tool, and finish without further tools. Do not include a precomputed answer.
5. Keep terminal failures as failures. Distinguish max-round exhaustion from provider error, missing terminal and missing structured submission in safe diagnostics and plain next-action messages. Do not accept a failed native process because a tool happened to submit output.

## Review and live acceptance

Source-review the config and every mirrored limit. Prepare a new immutable component using the normal installer; preserve the existing account home and selected model. A generic check must establish exactly one check-tool invocation and the matching structured value. Inspect its safe phase/round/terminal metadata and readiness evidence. Keep any remaining external blocker explicit; do not retry blindly or change spending authority.

Broad suites and release builds remain deferred. A passing generic check does not establish Tender-specific source review or full-provider support.

## Sources

- [Pinned access classification](https://github.com/xai-org/grok-build/blob/72a61251/crates/codegen/xai-grok-workspace/src/permission/types.rs#L246)
- [Pinned read-rule matching](https://github.com/xai-org/grok-build/blob/72a61251/crates/codegen/xai-grok-workspace/src/permission/policy.rs#L738)
- [Pinned read-only permission handling](https://github.com/xai-org/grok-build/blob/72a61251/crates/codegen/xai-grok-workspace/src/permission/manager/mod.rs#L2128)
- [Pinned turn budget](https://github.com/xai-org/grok-build/blob/72a61251/crates/codegen/xai-grok-shell/src/session/acp_session_impl/turn.rs#L3439)
- [Pinned terminal projection](https://github.com/xai-org/grok-build/blob/72a61251/crates/codegen/xai-grok-pager/src/headless.rs#L1302)
