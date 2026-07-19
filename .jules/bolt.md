## 2026-05-31 - Tauri Invoke Polling Causes React Re-renders
**Learning:** Tauri's `invoke` command returns new object references on every call, even if the data itself is perfectly identical. In an app heavily reliant on background polling (like Sonar, which polls multiple endpoints every 1-5 seconds), blindly passing these new objects into React's `setState` causes constant full-tree re-renders (reconciliations) while the app is supposedly idle.
**Action:** When polling Tauri endpoints with `invoke` and saving the result in state, use a functional state update with a deep equality check (e.g., `setState(prev => JSON.stringify(prev) === JSON.stringify(next) ? prev : next)`). Wrap independent sibling route components in `React.memo` so they aren't forced to re-render when a shared parent state (like a "last checked" timestamp) updates.

## 2026-07-19 - Fast Deep Equality vs JSON.stringify
**Learning:** `JSON.stringify` for object equality checking is extremely slow, especially when called frequently during polling. Creating a custom recursive deep equality check (`isEqual`) is over 75% faster.
**Action:** When performing equality checks in React state setters (or similar high-frequency code paths), favor a utility function like `isEqual` over `JSON.stringify` serialization.
