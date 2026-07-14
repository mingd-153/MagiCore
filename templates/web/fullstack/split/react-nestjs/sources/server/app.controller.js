import { Controller, Get } from "@nestjs/common";
@Controller()
export class AppController {
  constructor(appService) { this.appService = appService; }
  @Get("/api/health")
  health() { return { status: "ok" }; }
}
