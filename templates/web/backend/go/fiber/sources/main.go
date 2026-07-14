package main

import (
  "log"
  "os"
  "github.com/gofiber/fiber/v2"
)

func main() {
  port := os.Getenv("PORT")
  if port == "" { port = "3000" }
  app := fiber.New()
  app.Get("/api/health", HealthHandler)
  log.Printf("Listening on :%s", port)
  log.Fatal(app.Listen(":" + port))
}
