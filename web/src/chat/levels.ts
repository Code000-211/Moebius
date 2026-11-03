/**
 * The reasoning levels a chat's pill offers, which is the model's own list plus the level the chat is on already when
 * the model has no name for it. What the levels are, and what a level goes out as, is the server's doing; this only
 * shapes it for the pill.
 */
import { type AvailableModels, listedLevels, offeredLevels, sentLevel } from "../api/types/chat";
import type { PillOption } from "../ui/chat/PillSelect";

export function levelChoices(
  available: AvailableModels | undefined,
  model: string,
  current: string,
): PillOption[] {
  const levels = listedLevels(available, model);
  const options = offeredLevels(levels).map((level) => ({ value: level, label: level }));
  // A chat left on a level its model does not take keeps it in the list, saying what the endpoint is asked for.
  if (current !== "" && !options.some((option) => option.value === current)) {
    const sent = sentLevel(levels, current);
    const label = sent === current ? current : `${current} (sent as ${sent})`;
    options.push({ value: current, label });
  }
  return options;
}
