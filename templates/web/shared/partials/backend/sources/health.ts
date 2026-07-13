import { buildStatusPayload } from "../services/status";

export function healthRoute() {
  return buildStatusPayload("{{project_name}}");
}
