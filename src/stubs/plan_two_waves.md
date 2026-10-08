I'll split this in two: a feature for wave 1, then the docs that use it in wave 2.

```octobuddy-plan
{"estimate":{"rounds":6,"waves":2,"minutes":18},"slices":[{"slug":"feat-add","role":"developer","brief":"## Goal\nAdd `add(a, b)` to calc.py.\n\n## Files\n- calc.py\n\n## Done when\n- `add(1, 2)` returns 3","check":"grep -q '^def add' calc.py","rounds":3,"wave":1,"reviews":1},{"slug":"docs-write","role":"writer","brief":"## Goal\nDocument `add(a, b)` in README.md.\n\n## Files\n- README.md\n\n## Done when\n- README.md mentions add","check":"grep -q '^## add' README.md","rounds":2,"wave":2,"reviews":1}]}
```

Ready to start.