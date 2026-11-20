/** The Lessons entry of Settings: what the Retro learned from earlier cards, as a page of its own. */
import { PageHeader } from "../../ui/page/PageHeader";
import { Lessons } from "../retro/Lessons";

export function LessonsPage() {
  return (
    <>
      <PageHeader title="Lessons" />
      <Lessons />
    </>
  );
}
