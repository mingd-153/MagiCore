package com.{{ project_package }};

import jakarta.servlet.Filter;
import jakarta.servlet.FilterChain;
import jakarta.servlet.ServletException;
import jakarta.servlet.http.HttpServletRequest;
import jakarta.servlet.http.HttpServletResponse;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.web.cors.CorsConfiguration;
import org.springframework.web.cors.UrlBasedCorsConfigurationSource;
import org.springframework.web.filter.CorsFilter;
import org.springframework.web.util.ContentCachingResponseWrapper;

import java.io.IOException;
import java.util.UUID;

@Configuration
public class WebConfig {

    @Bean
    public CorsFilter corsFilter() {
        var config = new CorsConfiguration();
        config.addAllowedOrigin("*");
        config.addAllowedMethod("*");
        config.addAllowedHeader("*");
        config.addExposedHeader("X-Request-ID");
        var source = new UrlBasedCorsConfigurationSource();
        source.registerCorsConfiguration("/**", config);
        return new CorsFilter(source);
    }

    @Bean
    public Filter requestIdFilter() {
        return new Filter() {
            @Override
            public void doFilter(jakarta.servlet.ServletRequest servletRequest,
                                 jakarta.servlet.ServletResponse servletResponse,
                                 FilterChain chain) throws IOException, ServletException {
                var req = (HttpServletRequest) servletRequest;
                var res = (HttpServletResponse) servletResponse;
                var id = req.getHeader("X-Request-ID");
                if (id == null || id.isBlank()) {
                    id = UUID.randomUUID().toString();
                }
                res.setHeader("X-Request-ID", id);
                chain.doFilter(req, res);
            }
        };
    }
}
