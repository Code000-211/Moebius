/** The page a run links to (`/runs/<id>`): the live view is built in a later issue, so for now it names the run. */
import { Link, useParams } from "react-router";
import { Header } from "../shell/Page";
import { Placeholder } from "../shell/Placeholder";

export function RunPage() {
  const { id } = useParams();
  return (
    <>
      <Header breadcrumb="Runs" title={`Run ${id}`} />
      <Placeholder title={`The live view of run ${id}`} />
      <p className="mt-base text-sm">
        <Link to="/" className="text-accent hover:underline">
          Back
        </Link>
      </p>
    </>
  );
}
