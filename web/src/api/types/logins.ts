/** A person's logins on the forges (`GET /v1/logins`), the name on the forge without an `@`; empty for none. */
export interface Logins {
  github: string;
  gitlab: string;
  forgejo: string;
}
