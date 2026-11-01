/** The body of a page that is not built in the web app yet: the standard empty state, saying what is missing and where
 * it is today. */
import { Empty } from "../../ui/page/Empty";

export function Placeholder({ title }: { title: string }) {
  return (
    <Empty title={`${title} is coming to the web app soon`} body="The desktop app has it today." />
  );
}
