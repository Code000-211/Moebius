/** The Appearance entry of Settings: the theme choice of this browser, as a page of its own. */
import { PageHeader } from "../../ui/page/PageHeader";
import { Appearance } from "./Appearance";

export function AppearancePage() {
  return (
    <>
      <PageHeader title="Appearance" />
      <Appearance />
    </>
  );
}
