# The `http` node

Kind name: `http`. Makes one HTTP request and outputs the answer. The request is made by the Moebius server, since the
sandbox that runs [expressions](expressions.md) has no network.

```toml
[[nodes]]
id = "issue"
kind = "http"
params = { method = "GET", url = "https://api.example.com/items/{{ input.id }}", secret = "example-api", timeout_seconds = 10 }
```

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `url` | string | The URL. Required. May carry [expressions](expressions.md). |
| `method` | string | `GET` (the default), `POST`, `PUT`, `PATCH`, `DELETE` or `HEAD`; any case. |
| `headers` | table of strings | Headers to send. Their values are stored with the workflow, so they are not for credentials: use `secret`. |
| `body` | string or any JSON | A string is sent as it is. Any other value is sent as JSON, with `Content-Type: application/json` unless `headers` sets one. |
| `timeout_seconds` | number | Above 0 and at most 300. Default 30. |
| `secret` | string | The name of a secret kept in `<data_dir>/secrets.toml` (the Models page, or `"$MOEBIUS"`). |
| `secret_header` | string | The header the secret is sent in. Default `Authorization`. Needs `secret`. |
| `secret_prefix` | string | Written before the secret's value. Default `Bearer ` (with the space). Needs `secret`. |

Any other key is refused, and so is a `secret` that names nothing in the file (the node fails when it runs).

## Secrets

A credential is given only by name. The node reads the value from `secrets.toml` when it runs, sends it, and passes it
to the redaction that every store write, log line and web page goes through (`remember_secret`), so the value is
replaced by `[redacted:secret]` wherever it would be written, including in a response that echoes it. A node never
holds the value in its parameters. A secret shorter than the redaction's minimum length is sent but not redacted, so
use real keys.

## URL policy

The call is made only when all of these hold; otherwise the node fails, and no request is made:

- the scheme is `https` or `http`;
- the URL carries no user name or password;
- the host, in lower case, is one of [`workflows.http_hosts`](../configuration.md#workflows) in the server
  configuration. With none listed, every call is refused.

The failure reads `the URL policy refuses the call: <reason>`. Redirects are not followed (the answer with status 3xx
is the output), so a redirect cannot leave the allowed hosts.

## Ports

One input, `in`, and one output, `out`.

## Output

```json
{ "status": 200, "headers": { "content-type": "application/json" }, "body": { "items": [1, 2] } }
```

- `status` is the HTTP status code. A 4xx or 5xx answer is an output, not a failure; a later node can route on it.
- `headers` holds the response headers by lower-case name; a header sent several times is joined with `, `.
- `body` is the parsed JSON when the response's `Content-Type` contains `json` and the body parses; otherwise it is the
  body as text. A later node reads them as `input.body.items` or `node("issue").status`.

## Failure

The node fails, and no output is stored, when the URL policy refuses the call, the secret is not found or is not a
valid header value, a header is not valid, the connection fails, the request takes longer than `timeout_seconds`
(`the request timed out after <n> seconds`), or the body is longer than 10 MiB. Messages never include the URL's query
or a secret.

The module is `crates/moebius-core/src/workflows/nodes/http.rs`.
