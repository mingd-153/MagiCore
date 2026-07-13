package com.example.controller;

import com.example.config.AppConfig;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

import java.time.Instant;
import java.util.Map;

@RestController
public class HealthController {

    @Autowired
    private AppConfig appConfig;

    @GetMapping("/health")
    public Map<String, Object> health() {
        return Map.of(
            "status", "ok",
            "service", appConfig.getName(),
            "timestamp", Instant.now().toString()
        );
    }

    @GetMapping("/")
    public Map<String, Object> root() {
        return Map.of(
            "service", appConfig.getName(),
            "framework", appConfig.getFramework(),
            "message", "{{project_name}} backend scaffold ready"
        );
    }
}
