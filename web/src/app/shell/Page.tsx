/** The page for an address that matches nothing: a `PageHeader` and an empty state, like every other page. */
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";

/** An address that is not one of the app's pages. */
export function NotFoundPage() {
  return (
    <>
      <PageHeader title="Not found" />
      <Empty title="There is no such page" body="Pick one from the sidebar." />
    </>
  );
}
