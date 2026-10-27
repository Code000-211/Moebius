# Moebius is every harness's model provider

Every harness enters Moebius as its only model provider, and Moebius routes each model request to the real provider. The
provider keys therefore live in Moebius and nowhere else, which reverses the earlier rule that Moebius holds no keys. In
exchange, one place sees all model traffic: the token budget of a card is enforced there and not just reported, models and
fallbacks are chosen there, and every request and response becomes evidence for the Retro. MCP servers reach the harnesses
the same way, through Moebius acting as the one MCP server that bundles all the others; skills reach them through the harness
configuration of each run.

## Considered Options

- Each harness keeps its own provider logins (the earlier rule): nothing to secure in Moebius, but no budget it can enforce
  and no view of what the models were asked and answered.
