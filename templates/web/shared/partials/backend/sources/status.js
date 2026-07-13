export function buildStatusPayload(service) {
  return {
    status: "ok",
    service,
    timestamp: new Date().toISOString(),
  };
}
