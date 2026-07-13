package com.example;

import jakarta.ws.rs.GET;
import jakarta.ws.rs.Path;
import jakarta.ws.rs.Produces;
import jakarta.ws.rs.core.MediaType;
import org.eclipse.microprofile.config.inject.ConfigProperty;

import java.util.Map;

@Path("/")
public class GreetingResource {

    @ConfigProperty(name = "app.name", defaultValue = "{{project_name}}")
    String appName;

    @ConfigProperty(name = "app.framework", defaultValue = "quarkus")
    String framework;

    @GET
    @Produces(MediaType.APPLICATION_JSON)
    public Map<String, Object> root() {
        return Map.of(
            "service", appName,
            "framework", framework,
            "message", "{{project_name}} backend scaffold ready"
        );
    }
}
