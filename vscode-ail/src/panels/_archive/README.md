# Code Archive: Scheduled for Review

This folder contains code that has been superseded by new implementations but preserved for safe review and potential repurposing.

## Contents

### `unifiedPanelHtml.legacy.ts`

**Status:** Superseded by `getChatPanelHtml()` in `unifiedPanelHtml.ts`

**Reason:** The three-column "transaction logger" UI has been replaced with a chat-trope interface. The original `getUnifiedPanelHtml()` function is preserved here in case:
- The new chat UI needs architectural reference material
- Fallback to the old UI is required during a transition period
- Specific features from the old UI need to be ported back
- The 3-column view is resurrected as an optional advanced mode in future versions

**Implementation date:** Originally implemented in early vscode-ail development. Superseded 2026-04-04.

**Decision point:** After smoke testing the new chat UI and confirming all functionality works in the telemetry drawer, a human review should determine if this code should be:
1. Deleted permanently (low risk, UI is fully replaced)
2. Converted to a feature flag (medium effort, allows A/B testing)
3. Archived indefinitely (safe, preserves institutional knowledge)

## How to restore from archive

If you need to revert the chat UI redesign or restore the old function:

```typescript
// In vscode-ail/src/panels/unifiedPanelHtml.ts
// Uncomment and import from archive:
// export { getUnifiedPanelHtml } from './_archive/unifiedPanelHtml.legacy';

// Then use in MonitorViewProvider and UnifiedPanel as before
```

## Safe deletion checklist

Before deleting this archive folder, verify:
- [ ] All smoke tests passed with new chat UI
- [ ] No feature regressions reported in telemetry drawer
- [ ] Product team confirmed old UI is not needed
- [ ] At least one full release cycle has passed with new UI in production
- [ ] No open issues referencing the old layout

---

Archive created as part of PR for Trojan Horse chat interface redesign.
Preserves code integrity while enabling safe architectural evolution.
