# Agent rules

1. When something is ambiguous, follow the original project first:
   [Pixel Composer](https://github.com/Ttanasart-pt/Pixel-Composer) (node set, inputs, defaults, behavior, shader math).
   Cite the upstream file in the spec. Ask the user only when upstream gives no answer,
   or when it conflicts with a decision recorded in `TODO.md`. Do not guess.
2. `TODO.md` has a `Feedback` section. Every piece of user feedback goes there as a checkbox item.
   If working on an item raises an ambiguity, list the points to confirm as an indented list under that item.
3. Commit to git after every change.
4. Everything in the repository is written in English: docs, code, comments, commit messages.
5. Spec first. Before implementing an area, write or update its spec in `spec/` (one file per area).
   Code follows the spec. If code needs to differ, change the spec first.
6. Tests first. Each spec section ends with numbered test cases (given → expected).
   Write the tests from those cases before the code, see them fail, then implement until they pass.
   Every bug fix adds a test case to the spec and a test that reproduces it.

Feedback format:

```markdown
- [ ] The feedback, close to the user's own words
  - To confirm: the open question
```

The plan, the decisions and the milestones live in `TODO.md`. Specs live in `spec/`, see `spec/README.md`.
