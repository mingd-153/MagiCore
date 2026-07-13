package main

import (
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

func HealthHandler(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"status":    "ok",
		"service":   "{{project_name}}",
		"timestamp": time.Now().UTC().Format(time.RFC3339),
	})
}
