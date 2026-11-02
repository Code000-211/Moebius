/**
 * The first message, offered.
 *
 * A chat with nothing in it asks a harder question than a chat with a conversation in it: a blank field and a page
 * full of the server's machinery does not say what can be asked of it. These are the asks the Moebius agent answers
 * from what the server already knows, so the first thing a chat does is show what it can do. They put words in the
 * field rather than sending them: the ask is a beginning, not a commitment, and the person is the one who sends it.
 */
import { Button } from "../ui/components/Button";

/** Each is something the agent answers from what the server already knows. */
export const STARTERS = [
  "What is on the board right now?",
  "Why did the last run fail?",
  "Add a project from its git URL",
];

/** The row of them, for the empty place above a composer. */
export function Starters({ onPick }: { onPick: (text: string) => void }) {
  return (
    <ul className="m-0 flex flex-wrap gap-2 p-0" data-testid="chat-starters">
      {STARTERS.map((starter) => (
        <li key={starter} className="flex">
          <Button type="button" variant="secondary" size="sm" onClick={() => onPick(starter)}>
            {starter}
          </Button>
        </li>
      ))}
    </ul>
  );
}
