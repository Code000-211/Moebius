/** The Appearance entry of Settings: the theme choice of this browser, as a page of its own. */
import { Appearance } from "./Appearance";
import { AreaTitle } from "./parts";

export function AppearancePage() {
  return (
    <div className="max-w-3xl">
      <AreaTitle title="Appearance" />
      <Appearance />
    </div>
  );
}
