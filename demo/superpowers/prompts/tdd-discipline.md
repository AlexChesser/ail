# Test-Driven Development Discipline

## Objective

Ensure every piece of production code is proven correct by a test that was written first and observed to fail. This eliminates the class of bugs where tests pass trivially, test the wrong thing, or were retrofitted to match existing behavior.

## The Iron Law

NO PRODUCTION CODE WITHOUT A FAILING TEST FIRST.

Write code before the test? Delete it. Start over. No exceptions.

## Red-Green-Refactor Cycle

### RED — Write Failing Test
- One minimal test showing what should happen
- Clear name describing the behavior
- Real code, no mocks unless unavoidable

### Verify RED — Watch It Fail (MANDATORY)
- Run the test. Confirm it fails.
- Failure must be because the feature is missing, not because of typos
- If the test passes immediately, you're testing existing behavior — fix the test

### GREEN — Minimal Code
- Write the simplest code to pass the test
- Don't add features, refactor other code, or "improve" beyond the test

### Verify GREEN — Watch It Pass (MANDATORY)
- Run the test. Confirm it passes.
- Confirm other tests still pass.

### REFACTOR — Clean Up
- Only after green: remove duplication, improve names, extract helpers
- Keep tests green throughout

## Red Flags — STOP and Start Over

- Code written before test
- Test passes immediately (not testing anything new)
- Can't explain why the test failed
- Rationalizing "just this once"
- Proposing multiple fixes simultaneously

## Common Rationalizations (All Wrong)

- "Too simple to test" — Simple code breaks. Test takes 30 seconds.
- "I'll test after" — Tests passing immediately prove nothing.
- "TDD will slow me down" — TDD is faster than debugging.
- "Need to explore first" — Fine. Throw away exploration, start with TDD.
