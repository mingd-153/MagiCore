import "reflect-metadata";
import { NestFactory } from "@nestjs/core";
import { AppModule } from "./app.module.js";
const port = parseInt(process.env.PORT || "3000", 10);
const app = await NestFactory.create(AppModule);
await app.listen(port);
console.log(`running on :${port}`);
