package main

import "github.com/gin-gonic/gin"

func registerRoutes(r *gin.Engine) {
	r.GET("/health", healthCheck)
}

// healthCheck reports service readiness.
func healthCheck(c *gin.Context) {
	c.JSON(200, gin.H{"ok": true})
}
