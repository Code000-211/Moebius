/** The calls about the server: health, where it runs and whether it is ready for a first card. */
import { get } from "../http";
import type { Health, ServerInfo, Start } from "../types/server";

export function health(): Promise<Health> {
  return get<Health>("/v1/health");
}

export function serverInfo(): Promise<ServerInfo> {
  return get<ServerInfo>("/v1/server");
}

export function start(): Promise<Start> {
  return get<Start>("/v1/start");
}
