# Manager-led Tender workspace prototype

This is a disposable usability prototype for issue #59. It is deliberately separate from the production renderer and does not read or mutate real Tender data.

## Run it

```text
npm run prototype:workspace
```

The script opens Variant A. Variants B and C are available from the prototype switcher or with `?variant=B` and `?variant=C`. North Coast Medical Campus is the complete interactive scenario; the other Tender rows establish realistic catalogue context and are intentionally marked as previews.

## Moderated session

Ask the Tendering Engineer to complete these tasks without coaching:

1. Start or choose a Tender.
2. Check the source documents the Manager is using.
3. Answer the Manager's current question.
4. Review, revise, and approve the proposed plan.
5. Find what is working, what needs you, and what is done.
6. Open a working file and trace it back to the responsible Agent.
7. Inspect the shared Team room and one Agent's context, conversation, and outputs.
8. Correct an earlier decision after work has started and explain the reason.

Observe whether the Engineer can always answer four questions: where am I, what is happening, what needs me now, and where did this conclusion come from?

The optional **Show slow startup** control exists only to evaluate the startup state. Healthy startup remains silent.
