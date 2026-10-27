# Rules are enforced by Moebius, in the router

Moebius enforces the rules the work must follow itself, in the router every model response passes through: a tool call in a
response is checked before the harness receives it, so the same rules hold for every harness. It does not delegate this to a
harness extension such as pi-warden, which works only inside Pi, judges through an outside service (TypeSafe's Jev) unless it
runs offline, and improves its rules on its own (`/warden improve`), where Moebius leaves improvement to the Retro.

## Consequences

- The router has to hold back each tool call until it is complete and checked, while text still streams through.
- The router sees what a harness asks the model and what the model answers, not what the harness does on its own between
  model calls.
