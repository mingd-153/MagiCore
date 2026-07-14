import { Controller, Get } from "@nestjs/common";

@Controller()
export class AppController {
  @Get("/api/health")
  health() {
    return { status: "ok" };
  }
}
