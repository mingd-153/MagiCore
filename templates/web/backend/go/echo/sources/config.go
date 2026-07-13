package main

import "os"

type Config struct {
	Name      string
	Framework string
	Port      string
}

func LoadConfig() Config {
	port := os.Getenv("PORT")
	if port == "" {
		port = "3000"
	}
	return Config{
		Name:      "{{project_name}}",
		Framework: "echo",
		Port:      port,
	}
}
