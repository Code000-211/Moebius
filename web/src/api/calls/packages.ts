/**
 * The calls of the package sheet: the Moebius packages of the registries (`/v1/packages`) and the Pi packages of the
 * pi.dev catalog (`/v1/extensions`). A package name with a `/` (a scoped npm package) is written `%2F` in a path.
 */
import { del, get, post } from "../http";
import type {
  InstalledPackage,
  InstallRequest,
  PackageInfo,
  PackageOutcome,
  PackageSearch,
  PiHit,
  PiInfo,
  PiInstalled,
  PiOffered,
  PiUpdates,
} from "../types/packages";

const named = (name: string) => encodeURIComponent(name);

export function searchPackages(query: string): Promise<PackageSearch> {
  return get<PackageSearch>(`/v1/packages/search?q=${encodeURIComponent(query)}`);
}

export function installedPackages(): Promise<InstalledPackage[]> {
  return get<InstalledPackage[]>("/v1/packages");
}

export function packageInfo(name: string): Promise<PackageInfo> {
  return get<PackageInfo>(`/v1/packages/${named(name)}`);
}

export function installPackage(request: InstallRequest): Promise<PackageOutcome> {
  return post<PackageOutcome>("/v1/packages", request);
}

export function updatePackage(name: string): Promise<PackageOutcome> {
  return post<PackageOutcome>(`/v1/packages/${named(name)}/update`, {});
}

export function removePackage(name: string): Promise<PackageOutcome> {
  return del<PackageOutcome>(`/v1/packages/${named(name)}`);
}

/** The pi.dev catalog; `type` narrows it to one kind of package. */
export function searchPi(query: string, type: string | null): Promise<PiHit[]> {
  const kind = type === null ? "" : `&type=${encodeURIComponent(type)}`;
  return get<PiHit[]>(`/v1/extensions/search?q=${encodeURIComponent(query)}${kind}`);
}

export function piInfo(source: string): Promise<PiInfo> {
  return get<PiInfo>(`/v1/extensions/info?source=${encodeURIComponent(source)}`);
}

export function installedPi(): Promise<PiInstalled[]> {
  return get<PiInstalled[]>("/v1/extensions/packages");
}

/** Proposes installing a package, and naming it in `agent` when one is given. */
export function proposePi(source: string, agent: string | null): Promise<PiOffered> {
  return post<PiOffered>(
    "/v1/extensions/packages",
    agent === null ? { source } : { source, agent },
  );
}

export function updatePi(name: string): Promise<PiUpdates> {
  return post<PiUpdates>(`/v1/extensions/packages/${named(name)}/update`, {});
}

export function updateAllPi(): Promise<PiUpdates> {
  return post<PiUpdates>("/v1/extensions/update", {});
}

export function removePi(name: string): Promise<PiOffered> {
  return del<PiOffered>(`/v1/extensions/packages/${named(name)}`);
}
