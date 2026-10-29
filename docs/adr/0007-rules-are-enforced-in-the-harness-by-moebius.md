# Rules are enforced by Moebius, inside each harness

Moebius enforces the rules the work must follow itself. For each harness it ships a bridge, an extension of that harness
(for Pi-Bolt, a Pi extension loaded into every run), which hands every tool call to Moebius before the harness runs it; Moebius checks it and either lets
it run or blocks it with the rule it broke. The check sits inside the harness rather than in the router because only there are
the calls that a tool makes on its own visible, such as the many calls of one Pi-Bolt `codemode` script, and because a failing
handler there blocks the call instead of letting it through. For a harness that offers no such hook, the router checks the tool
calls in each model response instead, before the harness receives them. Moebius does not delegate enforcement to an extension
such as pi-warden, which works only inside Pi, judges through an outside service (TypeSafe's Jev) unless it runs offline, and
improves its rules on its own, where Moebius leaves improvement to the Retro.

## Consequences

- Every harness Moebius supports needs a bridge of its own, or falls back to the router's weaker check.
- The bridge also reports every tool result and error to Moebius, removes secrets from tool output before the model sees it,
  and passes questions the harness asks during a run to the decision model.
