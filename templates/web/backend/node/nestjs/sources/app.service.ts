import { Injectable } from "@nestjs/common";
import { loadConfig } from "./config/app.js";
import { healthRoute } from "./routes/health.js";

@Injectable()
export class AppService {
  private readonly config;

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
