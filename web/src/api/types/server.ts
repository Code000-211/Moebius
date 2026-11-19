/** Types of the calls about the server itself and the session; they follow docs/reference/desktop-api.md. */

/** `GET /v1/health`. */
export interface Health {
  version: string;
}

/** `GET /v1/server`: where the server runs. */
export interface ServerInfo {
  program: string;
  version: string;
  config_file: string;
  data_dir: string;
  source_dir?: string;
  api_address: string;
  router_address: string;
  host: string;
  os: string;
  arch: string;
}

/** A person of the organisation (`GET /v1/people`). */
export interface Person {
  id: number;
  /** Without the `@`. */
  handle: string;
  name: string;
  role: "owner" | "member" | "viewer";
  disabled: boolean;
  created_at: number;
}

/**
 * `GET /v1/session`: the answer of a signed-in browser, with the person it is signed in as. `person` is null for an
 * agent's run token, and absent for the moment between signing in and reading the session again.
 */
export interface Session {
  signed_in: true;
  person?: Person | null;
}

/**
 * The body of `POST /v1/session`: a person's access token or the server's token, or the one-time code of a `?login=`
 * link.
 */
export type SignIn = { token: string } | { code: string };

/** `GET /v1/start`: one step of the first start. */
export interface StartStep {
  key: string;
  title: string;
  done: boolean;
  detail: string;
}

/** `GET /v1/start`. */
export interface Start {
  done: boolean;
  steps: StartStep[];
  warnings: string[];
}
