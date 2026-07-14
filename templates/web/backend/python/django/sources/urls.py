from django.conf import settings
from django.http import JsonResponse
from django.urls import path

from .health import health_view
from .status import Status


def root(request):
    return JsonResponse({
        "service": settings.SERVICE_NAME,
        "framework": settings.SERVICE_FRAMEWORK,
        "message": "{{project_name}} backend scaffold ready",
    })


def status_view(request):
    return JsonResponse(Status().dict())


urlpatterns = [
    path("api/health", health_view, name="health"),
    path("api/status", status_view, name="status"),
    path("", root, name="root"),
]
