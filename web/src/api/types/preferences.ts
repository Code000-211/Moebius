/** What a person chose for themselves in the web app (`GET /v1/preferences`). */
export interface Preferences {
  /** Whether the thinking of a finished turn is shown open; a thought still arriving is always open. */
  show_thinking_expanded: boolean;
  /** Whether code blocks and long lines in chat answers, card threads and run pages wrap instead of scrolling sideways. */
  wrap_long_lines: boolean;
}
