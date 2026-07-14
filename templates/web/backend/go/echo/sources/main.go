package main

import (
  "log"
  "os"
  "github.com/labstack/echo/v4"
)

func main() {
  port := os.Getenv("PORT")
  if port == "" { port = "3000" }
  e := echo.New()
  e.GET("/api/health", HealthHandler)
  log.Printf("Listening on :%s", port)
  e.Logger.Fatal(e.Start(":" + port))
}
