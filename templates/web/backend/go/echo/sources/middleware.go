package main

import (
	"crypto/rand"
	"encoding/hex"
	"log/slog"
	"os"
	"time"

	"github.com/labstack/echo/v4"
)

func jsonLogger() echo.MiddlewareFunc {
	logger := slog.New(slog.NewJSONHandler(os.Stdout, nil))
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			start := time.Now()
			err := next(c)
			logger.Info("request",
				"method", c.Request().Method,
				"path", c.Request().URL.Path,
				"status", c.Response().Status,
				"latency", time.Since(start).String(),
				"request_id", c.Response().Header().Get("X-Request-ID"),
			)
			return err
		}
	}
}

func requestID() echo.MiddlewareFunc {
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c echo.Context) error {
			id := c.Request().Header.Get("X-Request-ID")
			if id == "" {
				b := make([]byte, 8)
				rand.Read(b)
				id = hex.EncodeToString(b)
			}
			c.Response().Header().Set("X-Request-ID", id)
			return next(c)
		}
	}
}
