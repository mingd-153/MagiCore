package com.{{ project_package }}.api;

import jakarta.ws.rs.container.ContainerRequestContext;
import jakarta.ws.rs.container.ContainerResponseContext;
import jakarta.ws.rs.container.ContainerResponseFilter;
import jakarta.ws.rs.ext.Provider;
import java.util.UUID;

@Provider
public class RequestIdFilter implements ContainerResponseFilter {

    @Override
    public void filter(ContainerRequestContext request, ContainerResponseContext response) {
        var id = request.getHeaderString("X-Request-ID");
        if (id == null || id.isBlank()) {
            id = UUID.randomUUID().toString();
        }
        response.getHeaders().putSingle("X-Request-ID", id);
    }
}
