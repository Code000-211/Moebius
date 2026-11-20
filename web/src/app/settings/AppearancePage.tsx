/** The Appearance entry of Settings: the theme choice of this browser, as a page of its own. */
import { Appearance } from "./Appearance";
import { AreaColumn, AreaTitle } from "./parts";

export function AppearancePage() {
  return (
    <AreaColumn>
      <AreaTitle title="Appearance" />
      <Appearance />
    </AreaColumn>
  );
}
