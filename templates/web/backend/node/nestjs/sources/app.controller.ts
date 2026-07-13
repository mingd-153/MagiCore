import { Controller, Get } from "@nestjs/common";
import { AppService } from "./app.service.js";

@Controller()
export class AppController {
  private readonly appService = new AppService();

  @Get()
  root() {
    return this.appService.getInfo();
  }

  @Get("health")
  async health() {
    return this.appService.health();
  }
}
