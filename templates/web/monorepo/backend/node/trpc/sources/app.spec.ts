import { describe, it, expect } from "vitest";
import { app } from "../src/lib/app.js";

describe("app", () => {
  it("exports app", () => { expect(app).toBeDefined(); });
});
