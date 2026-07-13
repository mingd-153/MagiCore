package com.example;

import jakarta.ws.rs.GET;
import jakarta.ws.rs.Path;
import jakarta.ws.rs.Produces;
import jakarta.ws.rs.core.MediaType;
import org.eclipse.microprofile.config.inject.ConfigProperty;

import java.time.Instant;
import java.util.Map;

@Path("/health")
public class HealthResource {

    @ConfigProperty(name = "app.name", defaultValue = "{{project_name}}")
    String appName;

    @GET
    @Produces(MediaType.APPLICATION_JSON)
    public Map<String, Object> health() {
        return Map.of(
            "status", "ok",
            "service", appName,
            "timestamp", Instant.now().toString()
        );
    }
}
