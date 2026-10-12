## Goal
Add `clamp(x, lo, hi)` to bounds.py: it clamps `x` to the range `[lo, hi]`.

## Files
- bounds.py

## Facts
- When `lo > hi` the range is empty: `clamp` must raise `ValueError` for it (not clamp, not swap the ends).
- test_bounds.py is the project's test file; OctoBuddy's check below tests more than it does.

## Done when
- `clamp(5, 0, 10) == 5`, `clamp(-1, 0, 10) == 0`, `clamp(11, 0, 10) == 10`
- `clamp(1, 10, 0)` raises `ValueError`
