# Task 27 — dynamic office test migration

The three obsolete inline-consultation tests now cover the approved dynamic
office lifecycle.

- The exact task/team approval guard verifies that the approved rationale
  reaches the existing Manager, while no `consult_specialist` callback,
  delegation grant, or staff assignment is created.
- The office integration path uses a real `PlanReviewService` and
  `JobManager`: the Manager creates a complete profile and work order, queues
  `execute_staff`, the controller runs the child after the Manager turn, and a
  later Manager turn reads the saved result and source before consolidation.
  The child finding remains an immutable staff draft; only the Manager finding
  is published.
- The SDK boundary uses a real PydanticAI `FunctionModel` with the approved
  route and a pinned Manager root. It scripts four Manager requests to create
  and queue staff, two scoped child requests returning the discriminated
  `StaffProviderOutput`, and three Manager requests to read and consolidate.
  The test checks `xhigh`, tool schema minimization, credential redaction,
  engineer rationale/root instruction in the child packet, nine aggregate
  requests with 900 input and 450 output tokens, source receipts, and draft
  publication boundaries.
- An in-process MCP worker regression exercises
  `AIWorkerClient.execute` through the real `Worker.operation("execute")`
  dispatch and confirms the worker receives the supported `execute` operation.
  It uses no provider, account, or customer data.

Validation from the isolated dynamic-office worktree:

```text
$env:PYTHONPATH='backend'; backend\.venv\Scripts\python.exe -m pytest backend/tests/test_integration_guards.py backend/tests/test_office.py backend/tests/test_office_sdk.py -q --tb=short
40 passed in 23.82s

backend\.venv\Scripts\python.exe -m ruff check backend/tests/test_integration_guards.py backend/tests/test_office.py backend/tests/test_office_sdk.py
All checks passed!
```

No production contract gap was found during this migration. The old inline
tool is absent, the current child operation is `execute`, and the worker
advertises and dispatches that operation. No commit was created.
