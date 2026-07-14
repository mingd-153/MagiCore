import { Controller, Get } from "@nestjs/common";
@Controller()
export class AppController {
  constructor(private readonly appService: AppService) {}
  @Get("/api/health")
  health() { return { status: "ok" }; }
}
