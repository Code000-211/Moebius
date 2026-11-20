/** What a person chose for themselves in the web app (`GET /v1/preferences`). */
export interface Preferences {
  /** Whether the thinking of a finished turn is shown open; a thought still arriving is always open. */
  show_thinking_expanded: boolean;
}
