package main

import (
	"crypto/rand"
	"encoding/hex"
	"log/slog"
	"os"
	"time"

	"github.com/gofiber/fiber/v2"
)

func jsonLogger() fiber.Handler {
	logger := slog.New(slog.NewJSONHandler(os.Stdout, nil))
	return func(c *fiber.Ctx) error {
		start := time.Now()
		err := c.Next()
		logger.Info("request",
			"method", c.Method(),
			"path", c.Path(),
			"status", c.Response().StatusCode(),
			"latency", time.Since(start).String(),
			"request_id", c.GetRespHeader("X-Request-ID"),
		)
		return err
	}
}

func requestID() fiber.Handler {
	return func(c *fiber.Ctx) error {
		id := c.Get("X-Request-ID")
		if id == "" {
			b := make([]byte, 8)
			rand.Read(b)
			id = hex.EncodeToString(b)
		}
		c.Set("X-Request-ID", id)
		return c.Next()
	}
}
