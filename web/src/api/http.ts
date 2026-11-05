/**
 * The one place that talks HTTP to the server: every call of `calls/` goes through `request`. Requests are same
 * origin, carry the session cookie and the `X-Moebius: 1` header the server wants on every request that has the cookie,
 * and a failed one becomes an `ApiError` with the status and the plain-text message the server answered.
 */

/** The status of an `ApiError` for a server that could not be reached at all. */
export const UNREACHABLE = 0;

/** A failed request: the status the server answered (`UNREACHABLE` when it did not) and its plain-text message. */
export class ApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }

  /** The server does not accept the session: the person has to sign in. */
  get unauthorized(): boolean {
    return this.status === 401;
  }

  /** No answer came: the server is down or the network is. */
  get unreachable(): boolean {
    return this.status === UNREACHABLE;
  }
}

export function isApiError(error: unknown): error is ApiError {
  return error instanceof ApiError;
}

export type Method = "GET" | "POST" | "PUT" | "DELETE";

export interface RequestOptions {
  /** A JSON body; the request then has `Content-Type: application/json`. */
  body?: unknown;
  signal?: AbortSignal;
  /** Where the requests go; tests pass a fake. */
  fetch?: typeof fetch;
}

/**
 * Calls `path` (such as `/v1/health`) and returns the parsed JSON answer, or undefined for an answer without a body
 * (204). Throws an `ApiError` for any status outside 200-299 and for a network failure.
 */
export async function request<T>(
  method: Method,
  path: string,
  options: RequestOptions = {},
): Promise<T> {
  const response = await send(method, path, options);
  if (response.status === 204) {
    return undefined as T;
  }
  const text = await response.text();
  if (text === "") {
    return undefined as T;
  }
  try {
    return JSON.parse(text) as T;
  } catch {
    throw new ApiError(response.status, "the server's answer is not JSON");
  }
}

/** Reads `path` as text, such as a plugin's page for its settings; it fails as `request` does. */
export async function getText(path: string, options: RequestOptions = {}): Promise<string> {
  return (await send("GET", path, options)).text();
}

/** Sends one request with the header and the cookie every call carries; an answer outside 200-299 is an `ApiError`. */
async function send(method: Method, path: string, options: RequestOptions): Promise<Response> {
  const headers: Record<string, string> = { "X-Moebius": "1" };
  const init: RequestInit = {
    method,
    headers,
    credentials: "same-origin",
    signal: options.signal,
  };
  if (options.body !== undefined) {
    headers["Content-Type"] = "application/json";
    init.body = JSON.stringify(options.body);
  }
  let response: Response;
  try {
    response = await (options.fetch ?? fetch)(path, init);
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw error;
    }
    throw new ApiError(UNREACHABLE, "cannot reach the server");
  }
  if (!response.ok) {
    throw new ApiError(response.status, await messageOf(response));
  }
  return response;
}

/** The plain-text body of a failed answer, or a sentence for its status when the body is empty. */
async function messageOf(response: Response): Promise<string> {
  const text = (await response.text().catch(() => "")).trim();
  return text !== "" ? text : `the server answered ${response.status}`;
}

export const get = <T>(path: string, options?: RequestOptions) => request<T>("GET", path, options);
export const post = <T>(path: string, body: unknown, options?: RequestOptions) =>
  request<T>("POST", path, { ...options, body });
export const put = <T>(path: string, body: unknown, options?: RequestOptions) =>
  request<T>("PUT", path, { ...options, body });
export const del = <T>(path: string, options?: RequestOptions) =>
  request<T>("DELETE", path, options);
