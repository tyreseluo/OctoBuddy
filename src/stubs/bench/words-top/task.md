## Goal
Add `top_words(text, n)` to words.py: the `n` most frequent words in `text`.

## Files
- words.py

## Facts
- Matching is case-insensitive; the result holds the words lowercased.
- Words with the same count come back in alphabetical order (a tie is not decided by where a word first appeared).
- Words are runs of letters; anything else separates them.
- test_words.py is the project's test file; OctoBuddy's check below tests more than it does.

## Done when
- `top_words("b a b a c", 2) == ["a", "b"]` (a tie, broken alphabetically)
- `top_words("Apple banana apple BANANA cherry", 2) == ["apple", "banana"]`
- `top_words("x y", 5) == ["x", "y"]` (fewer words than n is not an error)
