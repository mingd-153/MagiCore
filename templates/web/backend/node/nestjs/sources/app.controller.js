import { Controller, Get } from "@nestjs/common";
import { AppService } from "./app.service.js";
export class AppController {
  appService = new AppService();

  @Get()
  root() {
    return this.appService.getInfo();
  }

  @Get("health")
  async health() {
    return this.appService.health();
  }
}
