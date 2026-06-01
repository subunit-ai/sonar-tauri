## 2026-05-31 - Tauri Invoke Polling Causes React Re-renders
**Learning:** Tauri's `invoke` command returns new object references on every call, even if the data itself is perfectly identical. In an app heavily reliant on background polling (like Sonar, which polls multiple endpoints every 1-5 seconds), blindly passing these new objects into React's `setState` causes constant full-tree re-renders (reconciliations) while the app is supposedly idle.
**Action:** When polling Tauri endpoints with `invoke` and saving the result in state, use a functional state update with a deep equality check (e.g., `setState(prev => JSON.stringify(prev) === JSON.stringify(next) ? prev : next)`). Wrap independent sibling route components in `React.memo` so they aren't forced to re-render when a shared parent state (like a "last checked" timestamp) updates.
## 2025-03-09 - Avoid Unnecessary Clones with HashSet `contains`

**Learning:** When inserting elements into a `HashSet`, using a `.clone()` during `insert` will always cause a heap allocation even if the element already exists. Checking `contains` before cloning and inserting can yield significant performance improvements (e.g. from ~913ms to ~534ms in a tight loop with duplicates), despite the double-lookup overhead.

**Action:** Before calling `seen_pending.insert(row.id.clone())`, perform a check `if !seen_pending.contains(&row.id) { seen_pending.insert(row.id.clone()); ... }` to avoid memory allocation when the value is already present in the set.
