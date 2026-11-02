/**
 * What a person has typed but not sent, kept by place: one draft for each chat and one for the chat not started yet.
 * The page writes into React state while it is open and mirrors it here, so that leaving for another page and coming
 * back finds the half-written message where it was left. It lives for the tab, and a draft that is sent or cleared is
 * gone rather than kept.
 */
const drafts = new Map<string, string>();

/** The place a draft of a chat is kept under; "new" for a chat not started yet. */
export function draftKey(id: number | null): string {
  return id === null ? "new" : `chat:${id}`;
}

/** What was left unwritten at `key`, or nothing. */
export function readDraft(key: string): string {
  return drafts.get(key) ?? "";
}

/** Keep `text` as what stands at `key`; an empty text leaves no draft behind. */
export function saveDraft(key: string, text: string): void {
  if (text === "") drafts.delete(key);
  else drafts.set(key, text);
}
