/** The calls of the Improve pages: the Retro and its lessons, research, the decision model and the exports. */
import { del, get, post, put } from "../http";
import type {
  Brief,
  Export,
  KindCounts,
  Lesson,
  Proposal,
  Research,
  RetroSummary,
  RetroView,
  Shadowing,
} from "../types/improve";

/** The Retros, the latest day first. */
export function retros(): Promise<RetroSummary[]> {
  return get<RetroSummary[]>("/v1/retros");
}

export function retro(date: string): Promise<RetroView> {
  return get<RetroView>(`/v1/retros/${date}`);
}

/** Every proposal the Retros have made, oldest first — what an Inbox item names by its proposal id. */
export function proposals(): Promise<Proposal[]> {
  return get<Proposal[]>("/v1/proposals");
}

/** Runs today's Retro now. */
export function startRetro(): Promise<RetroView> {
  return post<RetroView>("/v1/retros", {});
}

/** The lessons of the Retro, the newest first, the retired ones too. */
export function lessons(): Promise<Lesson[]> {
  return get<Lesson[]>("/v1/lessons?retired=true");
}

export function editLesson(
  id: number,
  text: { situation: string; lesson: string; advice: string },
): Promise<Lesson> {
  return put<Lesson>(`/v1/lessons/${id}`, text);
}

export async function deleteLesson(id: number): Promise<void> {
  await del<unknown>(`/v1/lessons/${id}`);
}

/** All research, the newest first. */
export function researches(): Promise<Research[]> {
  return get<Research[]>("/v1/research");
}

/** Stores a brief as a draft. */
export function createResearch(repository: number, brief: Brief): Promise<Research> {
  return post<Research>("/v1/research", { repository, ...brief });
}

/** Writes the brief of a draft anew. */
export function editResearch(research: number, brief: Brief): Promise<Research> {
  return put<Research>(`/v1/research/${research}`, brief);
}

/** What a person does with a research. */
export type ResearchAction = "confirm" | "start" | "stop";

export function actOnResearch(research: number, action: ResearchAction): Promise<Research> {
  return post<Research>(`/v1/research/${research}/${action}`, {});
}

/** Deletes a research with its experiments, runs and lever branches; the server stops it first when it runs. */
export async function deleteResearch(research: number): Promise<void> {
  await del<unknown>(`/v1/research/${research}`);
}

/** Deletes a training-set export with its files on the server. */
export async function deleteExport(id: number): Promise<void> {
  await del<unknown>(`/v1/exports/${id}`);
}

/** How well the decision model agrees with the recorded choices, and its takeovers. */
export function shadowing(): Promise<Shadowing> {
  return get<Shadowing>("/v1/decision-model");
}

export function judgementKinds(): Promise<KindCounts[]> {
  return get<KindCounts[]>("/v1/judgements/kinds");
}

/** The training-set exports, the newest first. */
export function trainingExports(): Promise<Export[]> {
  return get<Export[]>("/v1/exports");
}

/** Writes an export of every kind on the server. */
export function startExport(): Promise<Export> {
  return post<Export>("/v1/exports", {});
}

/** Hands a kind the decision model took over back to the decider it had. */
export async function releaseTakeover(takeover: number): Promise<void> {
  await post<unknown>(`/v1/takeovers/${takeover}/release`, {});
}
