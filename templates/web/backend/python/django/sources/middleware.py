import uuid
import time
import logging
import json
from datetime import datetime, timezone

from django.http import HttpResponse


class JSONFormatter(logging.Formatter):
    def format(self, record):
        return json.dumps({
            "time": datetime.now(timezone.utc).isoformat(),
            "level": record.levelname,
            "message": record.getMessage(),
            "logger": record.name,
            "method": getattr(record, "method", None),
            "path": getattr(record, "path", None),
            "status": getattr(record, "status", None),
            "latency_ms": getattr(record, "latency_ms", None),
            "request_id": getattr(record, "request_id", None),
        })


handler = logging.StreamHandler()
handler.setFormatter(JSONFormatter())
logging.basicConfig(level=logging.INFO, handlers=[handler], force=True)
logger = logging.getLogger("django")


class RequestLoggingMiddleware:
    def __init__(self, get_response):
        self.get_response = get_response

    def __call__(self, request):
        if request.method == "OPTIONS":
            response = HttpResponse(status=204)
        else:
            request_id = request.META.get("HTTP_X_REQUEST_ID", str(uuid.uuid4()))
            request.request_id = request_id
            start = time.time()
            response = self.get_response(request)
            elapsed = time.time() - start
            response["X-Request-ID"] = request_id
            logger.info("request", extra={
                "method": request.method,
                "path": request.path,
                "status": response.status_code,
                "latency_ms": int(elapsed * 1000),
                "request_id": request_id,
            })
        response["Access-Control-Allow-Origin"] = "*"
        response["Access-Control-Allow-Headers"] = (
            "Content-Type, Authorization, X-Request-ID"
        )
        response["Access-Control-Expose-Headers"] = "X-Request-ID"
        return response
