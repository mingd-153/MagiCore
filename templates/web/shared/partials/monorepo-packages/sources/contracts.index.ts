import { z } from "zod";

export const launchSchema = z.object({
  workspace: z.string(),
  surface: z.string()
});

export type LaunchPayload = z.infer<typeof launchSchema>;
