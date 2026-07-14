package main

import (
  "log"
  "os"
  "github.com/gin-gonic/gin"
)

func main() {
  port := os.Getenv("PORT")
  if port == "" { port = "3000" }
  r := gin.Default()
  r.GET("/api/health", HealthHandler)
  log.Printf("Listening on :%s", port)
  r.Run(":" + port)
}
