import { describe, it, expect } from "vitest";
import { AppModule } from "../src/app.module.js";

describe("AppModule", () => {
  it("exports module", () => {
    expect(AppModule).toBeDefined();
  });
});
