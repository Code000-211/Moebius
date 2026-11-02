/**
 * The sentence a failed request shows. The server's own words are the best explanation of a refusal, so they are what
 * a page shows; the fallback is for a failure with no message of its own, such as a network that never reached it.
 */

/** What went wrong, in the server's words, or `fallback` when the failure carries no message. */
export function sayError(error: unknown, fallback: string): string {
  return error instanceof Error && error.message !== "" ? error.message : fallback;
}
