# Skills from Matt Pocock

These skills are copied unchanged from <https://github.com/mattpocock/skills> (MIT license, see `LICENSE`), at the
commit

    4ae2317ddb22b046e26455e4f9c9b3aaaf62212d

| Skill here | Path upstream | Given to |
| --- | --- | --- |
| `tdd` | `skills/engineering/tdd` | Coder |
| `diagnosing-bugs` | `skills/engineering/diagnosing-bugs` | Coder |
| `code-review` | `skills/engineering/code-review` | Reviewer |
| `grilling` | `skills/productivity/grilling` | Grilling partner |
| `to-tickets` | `skills/engineering/to-tickets` | Grilling partner |
| `wayfinder` | `skills/engineering/wayfinder` | Grilling partner |
| `domain-modeling` | `skills/engineering/domain-modeling` | Grilling partner |
| `research` | `skills/engineering/research` | Grilling partner |
| `prototype` | `skills/engineering/prototype` | Grilling partner |

Only the files Moebius uses are copied (`SKILL.md` and the files it links to); upstream's `agents/openai.yaml` files are
left out. Do not edit the copies, so that they can be replaced by a newer upstream commit. The skills are written for
work with a person: `moebius-autonomous` (in `../../moebius-autonomous/`) is loaded with them and says what to do where a
skill asks the user, and where it names an issue tracker or the setup skill. `wayfinder` calls the other three; the map
it keeps is a stack of Moebius's own issues, and `moebius-autonomous` has its "Wayfinding operations".

To update, copy the same paths from a newer commit and replace the commit above and `VENDORED_COMMIT` in
`src/harness/skill.rs` (a test checks that they agree). The Skills page shows the source and the commit.
