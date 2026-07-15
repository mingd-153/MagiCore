package main

import (
	"net/http"
	"time"

	"github.com/labstack/echo/v4"
)

func HealthHandler(c echo.Context) error {
	return c.JSON(http.StatusOK, map[string]interface{}{
		"status":    "ok",
		"service":   "{{project_name}}",
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	})
}
