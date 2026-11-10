/**
 * What a new chat offers to choose from: the agents it can be with, the models the router's upstreams serve (the model
 * classes first) and whether the server has a harness configuration repository, where a chat can work. All three are
 * read once and kept for a few minutes.
 */
import { useQuery } from "@tanstack/react-query";
import { availableModels, agents as readAgents, skills as readSkills } from "../api/calls/chat";
import { keys } from "../api/keys";
import type { Agent, AvailableModels, Chat } from "../api/types/chat";
import type { PillOption } from "../ui/chat/PillSelect";
import { levelChoices } from "./levels";

const FIVE_MINUTES = 5 * 60 * 1000;

/**
 * The agents a chat can be with: one whose files are broken is listed without a model and cannot be chosen; a built-in
 * one without a model runs on the model the chat is given, or the one the server falls back to.
 */
export function chatAgents(agents: Agent[]): Agent[] {
  return agents.filter((agent) => agent.model !== "" || (agent.built_in && !agent.error));
}

/** The ids of the models to offer: the model classes first, then the models, sorted and each once. */
export function modelChoices(available: AvailableModels | undefined): string[] {
  if (!available) return [];
  const models = [...new Set(available.models.map((model) => model.id))].sort();
  return [...available.classes.map((each) => each.name), ...models];
}

export function useChoices() {
  const agents = useQuery({ queryKey: keys.agents, queryFn: readAgents, staleTime: FIVE_MINUTES });
  const models = useQuery({
    queryKey: keys.models,
    queryFn: availableModels,
    staleTime: FIVE_MINUTES,
  });
  const skills = useQuery({ queryKey: keys.skills, queryFn: readSkills, staleTime: FIVE_MINUTES });
  return {
    agents: chatAgents(agents.data ?? []),
    models: modelChoices(models.data),
    available: models.data,
    hasHarness: (skills.data?.length ?? 0) > 0,
    loaded: agents.isSuccess,
  };
}

/** The model and the reasoning level a chat starts its agent on, and the choices to offer instead. */
export type ModelChoices = {
  model: string | null;
  thinking: string | null;
  modelOptions: PillOption[];
  levelOptions: PillOption[];
};

/** The model choices of one open chat, from the chat's own setting or its agent's default. */
export function chatModelChoices(chat: Chat, choices: ReturnType<typeof useChoices>): ModelChoices {
  const agent = choices.agents.find((each) => each.name === chat.agent);
  const model = chat.model ?? (agent?.model || null);
  const thinking = chat.thinking ?? agent?.thinking ?? null;
  const modelOptions: PillOption[] = [
    ...(model && !choices.models.includes(model) ? [{ value: model, label: model }] : []),
    ...choices.models.map((value) => ({ value, label: value })),
  ];
  // Only the levels the model's endpoint takes, plus this chat's own level when it has outlived the model.
  return {
    model,
    thinking,
    modelOptions,
    levelOptions: levelChoices(choices.available, model ?? "", thinking ?? ""),
  };
}

/** The model choices of the chat's agent and settings, read fresh. */
export function useChatModelChoices(chat: Chat): ModelChoices {
  const choices = useChoices();
  return chatModelChoices(chat, choices);
}
