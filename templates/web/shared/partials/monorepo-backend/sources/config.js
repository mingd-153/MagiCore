export function loadConfig() {
  return {
    service: "{{project_name}}-backend",
    framework: "{{backend_framework}}",
    port: Number(process.env.PORT ?? 4000)
  };
}
