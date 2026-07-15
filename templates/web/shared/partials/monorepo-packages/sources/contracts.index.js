export const launchSchema = {
  type: "object",
  required: ["workspace", "surface"],
  properties: {
    workspace: { type: "string" },
    surface: { type: "string" }
  }
};
