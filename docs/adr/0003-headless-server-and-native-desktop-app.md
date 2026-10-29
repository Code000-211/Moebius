# Moebius runs headless on a server; people use a native desktop app

Moebius runs permanently and headless on a server, so the workflow, the Retro and the trial window run whether or not anyone's
computer is on. People work with it through a desktop app that is native and GPU-rendered, written in Rust, with no browser
engine (neither Electron nor a system WebView), because Orca, built the other way, became laggy in use.

## Consequences

- The server is reachable only from a private network, and nothing on it is public. Forges therefore learn nothing by
  pushing to Moebius: the GitHub forge acts as its own GitHub App with the App's webhooks turned off and polls GitHub about
  every 30 seconds, because receiving webhooks would need an address GitHub can reach from the internet.
  [0012](0012-forge-webhooks-wake-the-poll.md) amends this: a forge that can reach the server may send webhooks, which only
  wake the poll.
