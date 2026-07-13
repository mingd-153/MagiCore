package main

import (
	"fmt"
	"log"

	"github.com/labstack/echo/v4"
)

func main() {
	cfg := LoadConfig()
	e := echo.New()

	e.GET("/health", HealthHandler)
	e.GET("/status", func(c echo.Context) error {
		return c.JSON(200, GetStatus())
	})
	e.GET("/", func(c echo.Context) error {
		return c.JSON(200, map[string]interface{}{
			"service":   cfg.Name,
			"framework": cfg.Framework,
			"message":   "{{project_name}} backend scaffold ready",
		})
	})

	addr := fmt.Sprintf(":%s", cfg.Port)
	log.Printf("Starting %s (echo) on %s", cfg.Name, addr)
	e.Logger.Fatal(e.Start(addr))
}
