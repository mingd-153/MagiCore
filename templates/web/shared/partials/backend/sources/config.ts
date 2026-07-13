export function loadConfig() {
  return {
    name: "{{project_name}}",
    framework: "{{backend_framework}}",
    port: Number(process.env.PORT ?? 3000)
  };
}
