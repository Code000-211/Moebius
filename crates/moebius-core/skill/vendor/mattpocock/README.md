# Skills from Matt Pocock

These skills are copied unchanged from <https://github.com/mattpocock/skills> (MIT license, see `LICENSE`), at the
commit

    59db609eb1e18aaf59b63284d0e52255aff0c63c

| Skill here | Path upstream | Given to |
| --- | --- | --- |
| `tdd` | `skills/engineering/tdd` | Coder |
| `diagnosing-bugs` | `skills/engineering/diagnosing-bugs` | Coder |
| `code-review` | `skills/engineering/code-review` | Reviewer |
| `grilling` | `skills/productivity/grilling` | Grilling partner |
| `to-tickets` | `skills/engineering/to-tickets` | Grilling partner |

Only the files Moebius uses are copied (`SKILL.md` and the files it links to); upstream's `agents/openai.yaml` files are
left out. Do not edit the copies, so that they can be replaced by a newer upstream commit. The skills are written for
work with a person: `moebius-autonomous` (in `../../moebius-autonomous/`) is loaded with them and says what to do where a
skill asks the user, and where it names an issue tracker or the setup skill.

To update, copy the same paths from a newer commit and replace the commit above and `COMMIT` in
`src/harness/skill.rs` (a test checks that they agree). The Skills page shows the source and the commit.
