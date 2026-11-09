/**
 * What the server's events do to the TanStack Query cache. The reference says the snapshot is read again on every
 * event, so an event that changes what the snapshot shows invalidates it; `card_waiting` is also patched in place,
 * which shows the reason at once. A chat's list follows the stream the same way, patched in place
 * (`patchChats`), and a chat's deletion is not read again for it: the patch is the whole of what the list needs, and
 * a read that left before the deletion and lands after it would put the deleted chat back with nothing to take it out. What is read about one card (history, runs) goes stale with the snapshot, and the
 * column scripts with `script_changed`, and a project's settings with `project_settings_changed`. A new page that reads something the events change adds its key to `keys.ts` and a
 * line to `invalidates` below (the package sheet is in `packagePages`; the pages of Rules, Automations, Retro, Experiments and Decision model are in `improvePages`). A workflow run's events make the workflow list, the definition it belongs to and its own run stale (`workflowPages`). Events are not replayed, so when the socket connects all that is shown is read again.
 */
import type { QueryClient, QueryKey } from "@tanstack/react-query";
import { keys } from "../api/keys";
import type { Snapshot } from "../api/types/board";
import type { Chat } from "../api/types/chat";
import type { ServerEvent } from "../api/types/events";
import { applyChatEvent, listChanged } from "../chat/chatList";

/** The kinds that change what the snapshot holds: repositories, cards, changes, runs of cards and the inbox. */
const SNAPSHOT_KINDS = new Set([
  "repository_added",
  "repository_updated",
  "repository_removed",
  "card_added",
  "card_updated",
  "card_deleted",
  "card_moved",
  "card_waiting",
  "card_parent_changed",
  "card_labels_changed",
  "card_blocking_changed",
  "card_claimed",
  "card_released",
  "card_commented",
  "card_closed",
  "card_reopened",
  "change_offered",
  "change_updated",
  "change_merged",
  "change_closed",
  "attempt_judged",
  "attempts_settled",
  "run_started",
  "run_ended",
  "grilling_ended",
  "grilling_published",
  "chat_deleted",
  "research_deleted",
  "grilling_deleted",
  "proposal_status",
  "takeover_proposed",
  "takeover_status",
  "retro_done",
]);

/** The kinds after which what a change or a run changed may have moved. */
const DIFF_KINDS = new Set([
  "change_offered",
  "change_updated",
  "change_merged",
  "change_closed",
  "run_ended",
]);

/** The keys of the cache that `event` makes stale. `lagged` means events were lost, so everything is. */
export function invalidates(event: ServerEvent): QueryKey[] {
  if (event.kind === "lagged") return [[]];
  const stale: QueryKey[] = [];
  if (SNAPSHOT_KINDS.has(event.kind)) stale.push(keys.snapshot, keys.cards);
  if (event.kind === "script_changed") stale.push(keys.scripts, keys.projectSettingsAll);
  if (event.kind === "project_settings_changed") {
    stale.push(keys.projectSettingsAll);
    // The plugins' settings of a repository are files of its settings (`plugins/<plugin>`).
    if (String(event.data.file).startsWith("plugins/")) {
      stale.push(keys.pluginProjectSettings(Number(event.data.repository)));
    }
  }
  // A repository's settings tell which `.moebius/` files are left, so they are read again with its files.
  if (event.kind === "repository_added" || event.kind === "repository_removed") {
    stale.push(keys.start);
  }
  if (event.kind === "run_started" || event.kind === "run_ended") stale.push(keys.runs);
  if (DIFF_KINDS.has(event.kind)) stale.push(keys.diffs);
  if (event.kind === "port_opened" || event.kind === "port_closed") stale.push(keys.ports);
  if (event.kind === "run_ended") stale.push(keys.usage);
  if (event.kind === "file_changed" && typeof event.data.repository === "number") {
    stale.push(keys.files(event.data.repository));
  }
  // A chat's deletion is put into the list of chats in place (applyChatEvent), and no read is made again for it: a
  // read that left before the deletion and lands after it would put the deleted chat back, and nothing would take
  // it out again — no event follows it. The patch is the whole of what the list needs, so the list is left alone.
  if (event.kind === "chat_deleted") {
    return stale;
  }
  if (listChanged(event)) stale.push(keys.chats);
  stale.push(...improvePages(event));
  stale.push(...packagePages(event));
  stale.push(...workflowPages(event));
  return stale;
}

/** What the Workflows list, a definition and a run read, by the run and node events that change it. */
function workflowPages(event: ServerEvent): QueryKey[] {
  if (!event.kind.startsWith("workflow_")) return [];
  const run = event.data.run;
  const stale: QueryKey[] = [keys.workflows, keys.workflowDefinitions];
  if (typeof run === "number") stale.push(keys.workflowRun(run));
  return stale;
}

/** What the pages of Rules, Automations, Retro, Experiments and Decision model read, by the events that change it. */
function improvePages(event: ServerEvent): QueryKey[] {
  switch (event.kind) {
    case "retro_done":
    case "trial_done":
      return [keys.retros, keys.lessons, keys.proposals];
    case "proposal_status":
      return [keys.retros, keys.lessons, keys.rules, keys.proposals];
    case "research_status":
    case "experiment_done":
    case "research_deleted":
      return [keys.research];
    case "export_deleted":
      return [keys.exports];
    case "takeover_proposed":
    case "takeover_status":
      return [["decision-model"]];
    case "automation_fired":
    case "run_ended":
      return [keys.automations];
    case "project_settings_changed":
      // Only the rules file of a project is read by these pages.
      return String(event.data.file) === "rules.md" ? [keys.rules] : [];
    default:
      return [];
  }
}

/**
 * What the package sheet and the Plugins page read, by the events that change it: a package installed, updated or
 * removed or turned off or on, an update the nightly check found (for a Pi package it is an inbox item, which the snapshot holds and
 * the sheet reads to mark the package), and a registry it could not read.
 */
function packagePages(event: ServerEvent): QueryKey[] {
  switch (event.kind) {
    case "package_installed":
    case "package_removed":
    case "package_check_error":
      return [keys.packages, keys.plugins];
    case "package_update_available":
    case "extension_update_available":
      return [keys.packages, keys.plugins, keys.snapshot];
    // A plugin turned off or on changes its row, its settings boxes (here and on the projects' pages) and its tag in the sheet.
    case "plugin_enabled_changed":
      return [keys.plugins, keys.packages, ["plugin-project-settings"]];
    default:
      return [];
  }
}

/** Puts the reason of `card_waiting` into the snapshot in the cache, if one is there. */
export function patchSnapshot(queryClient: QueryClient, event: ServerEvent): void {
  if (event.kind !== "card_waiting") return;
  const card = event.data.card;
  const reason = event.data.reason;
  if (typeof card !== "number") return;
  queryClient.setQueryData<Snapshot>(keys.snapshot, (snapshot) => {
    if (!snapshot) return snapshot;
    return {
      ...snapshot,
      boards: snapshot.boards.map((board) => {
        if (!board.cards.some((c) => c.id === card)) return board;
        const waiting = { ...board.waiting };
        if (typeof reason === "string") waiting[String(card)] = reason;
        else delete waiting[String(card)];
        return { ...board, waiting };
      }),
    };
  });
}

/** Puts what a chat's run event says (its status, its answer) into the list of chats in the cache, if one is there. */
export function patchChats(queryClient: QueryClient, event: ServerEvent): void {
  queryClient.setQueryData<Chat[]>(keys.chats, (chats) =>
    chats ? applyChatEvent(chats, event) : chats,
  );
}

/** Applies one event to the cache. */
export function applyEvent(queryClient: QueryClient, event: ServerEvent): void {
  patchSnapshot(queryClient, event);
  patchChats(queryClient, event);
  for (const queryKey of invalidates(event)) {
    void queryClient.invalidateQueries({ queryKey });
  }
}

/** The socket connected (again): what is shown may have missed events, so every read is made again. */
export function applyConnected(queryClient: QueryClient): void {
  void queryClient.invalidateQueries({ predicate: (query) => query.queryKey[0] !== "session" });
}
