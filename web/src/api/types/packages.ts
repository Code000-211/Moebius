/**
 * The package managers, as docs/reference/desktop-api.md writes them: the Moebius packages (`/v1/packages`: plugins,
 * workflows, rules and skills from the registries) and the Pi packages (`/v1/extensions`: the pi.dev catalog).
 */
import type { Change } from "./board";

export type PackageKind = "plugin" | "workflow" | "rules" | "skills";

/** A package found in the registries; `installed` is the installed version or null. */
export interface PackageSummary {
  name: string;
  kind: PackageKind;
  version: string;
  description: string;
  registry: string;
  compatible: boolean;
  installed: string | null;
}

/** A registry that could not be read; the others are answered all the same. */
export interface RegistryProblem {
  registry: string;
  message: string;
}

export interface PackageSearch {
  packages: PackageSummary[];
  problems: RegistryProblem[];
}

export interface PackageFile {
  to: string;
  url?: string;
  sha256?: string;
}

/** One version of a package in the registry, with whether it works with this Moebius. */
export interface PackageVersion {
  version: string;
  kind: PackageKind;
  description: string;
  license?: string;
  homepage?: string;
  moebius: string;
  compatible: boolean;
  targets?: string[];
  plugin?: { kind: string; program: string; decides?: string[]; launches?: boolean };
  files?: PackageFile[];
}

/** An entry of the lock: what is installed. */
export interface LockedPackage {
  name: string;
  version: string;
  kind: PackageKind;
  registry: string;
  program?: string;
  repository?: string;
  /** The change that proposed the files of a workflow, rules or skills package. */
  change?: number;
  files?: PackageFile[];
  rules?: string[];
}

/** An installed package with the newest version the nightly check found, if it is newer. */
export interface InstalledPackage extends LockedPackage {
  update_available: string | null;
  /** For a plugin: false when it is turned off. */
  enabled?: boolean;
}

export interface PackageInfo {
  name: string;
  registry: string;
  installed: LockedPackage | null;
  versions: PackageVersion[];
}

export interface InstallRequest {
  name: string;
  version?: string;
  registry?: string;
  repository?: string;
}

export interface PluginProcessStatus {
  program: string;
  state: string;
  restarts: number;
  error: string | null;
}

/** What an install, update or removal did. */
export interface PackageOutcome {
  name: string;
  version: string;
  kind: PackageKind;
  action: "installed" | "updated" | "removed" | "unchanged";
  restart_needed: boolean;
  /** The change that proposes the files of a workflow, rules or skills package. */
  change?: number;
  plugin?: PluginProcessStatus;
  leftover?: string;
}

export type PiKind = "extension" | "skill" | "theme" | "prompt";

/** A package of the pi.dev catalog. */
export interface PiHit {
  name: string;
  version: string;
  description: string;
  kinds: string[];
  weekly_downloads: number | null;
  npm: string | null;
  homepage: string | null;
  repository: string | null;
  date: string | null;
}

export interface PiResources {
  extensions: string[];
  skills: string[];
  prompts: string[];
  themes: string[];
}

export interface PiInfo {
  name: string;
  source: string;
  version: string;
  resolved: string;
  integrity: string | null;
  description: string;
  resources: PiResources;
  dependencies: string[];
  install_scripts: string[];
  files: number;
  installed: string | null;
}

/** A package the harness configuration repository vendors. */
export interface PiInstalled {
  name: string;
  source: string;
  version: string;
  resolved: string;
  integrity: string | null;
  sha256: string | null;
  resources: PiResources;
  install_scripts: string[];
  agents: string[];
  error: string | null;
}

/** A change that adds, updates or removes a Pi package, waiting for a person to merge it. */
export interface PiOffered {
  change: Change;
  package: string;
  action: "add" | "update" | "remove";
  source: string;
  from: string | null;
  to: string | null;
  resources: PiResources;
  agents: string[];
  changelog: string;
  files: string[];
  dependency_files: number;
  diff: string;
}

export interface PiUpdates {
  offered: PiOffered[];
  up_to_date: string[];
  failed: Array<{ package: string; error: string }>;
}
