/** One message of the event WebSocket `GET /v1/events`: `{"kind", "data"}`, see "Events" in the desktop API reference. */
export interface ServerEvent {
  kind: string;
  data: Record<string, unknown>;
}
