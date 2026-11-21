/** The page behind a line of the legacy table: waits for the projects, then replaces the address with the new one. */
import { useQuery } from "@tanstack/react-query";
import { Navigate, useLocation, useParams } from "react-router";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { NotFoundPage } from "../shell/Page";
import type { LegacyRoute } from "./redirects";

export function LegacyRedirect({ route }: { route: LegacyRoute }) {
  const params = useParams();
  const { search } = useLocation();
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  if (projects.isError)
    return (
      <ReadFailed
        error={projects.error}
        fallback="The projects could not be read. Try again."
        busy={projects.isFetching}
        onRetry={() => void projects.refetch()}
      />
    );
  if (!projects.data) return <SkeletonLines lines={2} message="Opening the page" />;
  const to = route.resolve(params, projects.data, new URLSearchParams(search));
  return to === null ? <NotFoundPage /> : <Navigate to={to} replace />;
}
