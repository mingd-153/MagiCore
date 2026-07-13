import { buildStatusPayload } from "../services/status.js";

export function healthRoute() {
  return buildStatusPayload("{{project_name}}");
}
