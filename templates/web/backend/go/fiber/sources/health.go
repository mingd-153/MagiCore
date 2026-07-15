package main

import (
	"time"

	"github.com/gofiber/fiber/v2"
)

func HealthHandler(c *fiber.Ctx) error {
	return c.JSON(fiber.Map{
		"status":    "ok",
		"service":   "{{project_name}}",
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	})
}
