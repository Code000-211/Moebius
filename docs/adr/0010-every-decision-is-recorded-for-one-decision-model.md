# Every decision is recorded, to train one decision model

Moebius is built to train its own decision model, so every decision point records a judgement, whoever decides there: a
tool-call check, an answer to a harness's question, a step of the workflow (did the review pass, back to coding, give up on a
stuck card, which model and budget for a card), triage of an outside issue, what the Retro picks to propose and to try, and
every decision a person makes (merging or rejecting a change, approving an issue, accepting or rejecting a proposal with the
reason, steering or stopping a run). This is decided now because data that is not recorded cannot be recovered later. All
judgements share one format (the kind of decision, its full context, the options, the choice, the reason and, later, the
outcome), and one model is trained on all of them, with the kind as part of its input. A person's decisions are the scarcest
and most valuable labels, because the person is the one the model is meant to stand in for.

## Consequences

- Judgements keep their full context, stored as content-addressed blocks so that what repeats costs almost nothing, with
  secrets removed when they are captured. They stay on the server and leave it only in an export a person starts.
- A trained decision model first runs in shadow and takes over one kind of decision at a time, only once it decides that kind
  at least as well as whoever decides now on judgements held back from training; the takeover is a proposal a person merges.
- Recording is part of the core's store (ADR 0009); exporting and training are plugins.
