from datetime import datetime, timezone

from django.conf import settings
from django.http import JsonResponse


def health_view(request):
    return JsonResponse({
        "status": "ok",
        "service": settings.SERVICE_NAME,
        "timestamp": datetime.now(timezone.utc).isoformat(),
    })
