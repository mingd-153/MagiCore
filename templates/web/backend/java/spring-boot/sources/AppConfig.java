package com.example.config;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.context.annotation.Configuration;

@Configuration
public class AppConfig {

    @Value("${app.name:{{project_name}}}")
    private String name;

    @Value("${app.framework:spring-boot}")
    private String framework;

    public String getName() {
        return name;
    }

    public String getFramework() {
        return framework;
    }
}
