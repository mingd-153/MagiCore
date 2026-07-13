from django.conf import settings
from django.http import JsonResponse
from django.urls import path

from .routes.health import health_view
from .services.status import Status


def root(request):
    return JsonResponse({
        "service": settings.SERVICE_NAME,
        "framework": settings.SERVICE_FRAMEWORK,
        "message": "{{project_name}} backend scaffold ready",
    })


def status_view(request):
    return JsonResponse(Status().dict())


urlpatterns = [
    path("health", health_view, name="health"),
    path("status", status_view, name="status"),
    path("", root, name="root"),
]
