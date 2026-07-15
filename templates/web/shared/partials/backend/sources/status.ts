export function buildStatusPayload(service: string) {
  return {
    status: "ok",
    service,
    timestamp: new Date().toISOString(),
  };
}
