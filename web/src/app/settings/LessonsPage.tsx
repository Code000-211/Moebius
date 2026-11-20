/** The Lessons entry of Settings: what the Retro learned from earlier cards, as a page of its own. */
import { Lessons } from "../retro/Lessons";
import { AreaTitle } from "./parts";

export function LessonsPage() {
  return (
    <div className="max-w-3xl">
      <AreaTitle title="Lessons" />
      <Lessons />
    </div>
  );
}
