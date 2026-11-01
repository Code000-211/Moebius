/** Where a chat works, named: the repository the snapshot has, or the harness configuration repository. */
import type { Snapshot } from "../api/types/board";
import type { Chat } from "../api/types/chat";

/** How the harness configuration repository is called where a chat's place is named. */
export const HARNESS_PLACE = "harness configuration";

export function placeName(
  snapshot: Snapshot | undefined,
  chat: Pick<Chat, "harness_repository" | "repository_id">,
): string | null {
  if (chat.harness_repository) return HARNESS_PLACE;
  const board = snapshot?.boards.find((each) => each.repository.id === chat.repository_id);
  return board?.repository.name ?? null;
}

/** The change a chat offered, found by its branch among the board's changes. */
export function offeredChange(snapshot: Snapshot | undefined, chat: number) {
  const branch = `moebius/chat-${chat}`;
  const changes = [
    ...(snapshot?.boards.flatMap((board) => board.changes) ?? []),
    ...(snapshot?.harness_changes ?? []),
  ];
  return changes.findLast((change) => change.branch === branch);
}
