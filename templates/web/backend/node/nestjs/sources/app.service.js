import { Injectable } from "@nestjs/common";
import { loadConfig } from "./config/app.js";
import { healthRoute } from "./routes/health.js";

export class AppService {
  constructor() {
    this.config = loadConfig();
  }

  getInfo() {
    return {
      service: this.config.name,
      framework: this.config.framework,
      mode: "fullstack",
      message: "MegaGate fullstack API scaffold ready",
    };
  }

  async health() {
    return healthRoute();
  }
}
