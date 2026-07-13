package main

import (
	"fmt"
	"log"

	"github.com/gin-gonic/gin"
)

func main() {
	cfg := LoadConfig()
	r := gin.Default()

	r.GET("/health", HealthHandler)
	r.GET("/status", func(c *gin.Context) {
		c.JSON(200, GetStatus())
	})
	r.GET("/", func(c *gin.Context) {
		c.JSON(200, gin.H{
			"service":   cfg.Name,
			"framework": cfg.Framework,
			"message":   "{{project_name}} backend scaffold ready",
		})
	})

	addr := fmt.Sprintf(":%s", cfg.Port)
	log.Printf("Starting %s (gin) on %s", cfg.Name, addr)
	r.Run(addr)
}
