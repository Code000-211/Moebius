# A forge's webhook wakes the poll; it is never trusted as data

Amends [0003](0003-headless-server-and-native-desktop-app.md), which has the forges polled because the server sits in a
private network. Where a forge can reach the server, a webhook saves the wait of up to a poll period, so the server
accepts one, optionally.

## Consequences

- A forge is set up for webhooks by storing a secret named `webhook-github`, `webhook-gitlab` or `webhook-forgejo`, which
  the server makes on request (the Settings page of the app has the button) or which a person stores with `PUT
  /v1/secrets/{name}`. Without the secret the endpoint answers 404; no key of `moebius.toml` is needed and the secret
  is never in that file. It is redacted like every other secret.
- `POST /v1/webhooks/{forge}` is the one route of the desktop API that needs no token, since a forge cannot send one. It
  does two things only: it checks the delivery against the secret (GitHub's `X-Hub-Signature-256` and Forgejo's
  `X-Gitea-Signature`, both an HMAC-SHA256 of the body, and GitLab's `X-Gitlab-Token`, compared in constant time) and
  asks the poll of that forge to run at once. A delivery that does not check out is answered with 401 and changes
  nothing.
- A delivery is not data. Nothing in its body reaches a card, a change or the store; at most it names the repository
  whose poll is asked for, and a name that is no repository of the forge polls nothing. The poll stays the one source of
  truth, so a lost, repeated, reordered or forged-but-verified delivery cannot make the board wrong.
- Polling goes on at its usual pace as the safety net for the deliveries that never arrive. A server that no forge can
  reach works exactly as before.
