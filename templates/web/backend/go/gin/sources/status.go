package main

import "runtime"

type Status struct {
	Service   string `json:"service"`
	Version   string `json:"version"`
	GoVersion string `json:"go_version"`
	Uptime    string `json:"uptime"`
}

func GetStatus() Status {
	return Status{
		Service:   "{{project_name}}",
		Version:   "0.1.0",
		GoVersion: runtime.Version(),
	}
}
