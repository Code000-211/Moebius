/** The Lessons tab of Improve: what the Retro learned from earlier cards, as a page of its own. */
import { PageHeader } from "../../ui/page/PageHeader";
import { ImproveTabs } from "../improve/ImproveTabs";
import { Lessons } from "./Lessons";

export function LessonsPage() {
  return (
    <div className="max-w-3xl">
      <PageHeader title="Improve" tabs={<ImproveTabs value="improve/lessons" />} />
      <Lessons />
    </div>
  );
}
