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

/** `GET /v1/session`: the answer of a signed-in browser. */
export interface Session {
  signed_in: true;
}

/** The body of `POST /v1/session`: the server's token, or the one-time code of a `?login=` link. */
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
