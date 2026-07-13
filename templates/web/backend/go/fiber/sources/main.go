package main

import (
	"fmt"
	"log"

	"github.com/gofiber/fiber/v2"
)

func main() {
	cfg := LoadConfig()
	app := fiber.New()

	app.Get("/health", HealthHandler)
	app.Get("/status", func(c *fiber.Ctx) error {
		return c.JSON(GetStatus())
	})
	app.Get("/", func(c *fiber.Ctx) error {
		return c.JSON(fiber.Map{
			"service":   cfg.Name,
			"framework": cfg.Framework,
			"message":   "{{project_name}} backend scaffold ready",
		})
	})

	addr := fmt.Sprintf(":%s", cfg.Port)
	log.Printf("Starting %s (fiber) on %s", cfg.Name, addr)
	log.Fatal(app.Listen(addr))
}
