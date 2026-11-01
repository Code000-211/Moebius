/** Types of the calls about `moebius.toml` and the webhooks; they follow docs/reference/desktop-api.md, `GET /v1/settings`. */

/** One key of the configuration: what the file says (or the default), and whether that is the default. */
export interface SettingValue {
  value: unknown;
  default: boolean;
}

/** `GET /v1/settings`. */
export interface ServerSettings {
  file: string;
  values: Record<string, SettingValue>;
  secrets: string[];
  restart_needed: boolean;
  restart_keys: string[];
}

/** The answer of `PUT /v1/settings` and `POST /v1/harness/setup`. */
export interface SavedSettings {
  restart_needed: boolean;
  restart_keys: string[];
}

/** The body of `PUT /v1/settings`. */
export interface SettingsChange {
  set?: Record<string, unknown>;
  remove?: string[];
}

/** An entry of `GET /v1/webhooks`. */
export interface Webhook {
  forge: string;
  path: string;
  header: string;
  secret: string | null;
}
