## 2025-03-08 - [Optimize Array Element Removal]
**Learning:** Using `findIndex` combined with `splice` on a cloned array is significantly faster than using `filter` when removing a single item from a large array, as it stops iterating once the item is found (O(N) best/average case vs O(N) worst case for filter).
**Action:** Default to `findIndex` and `splice` (or `toSpliced` if supported) over `filter` when the intent is to remove exactly one element from a large array, especially in frequent state updates.
