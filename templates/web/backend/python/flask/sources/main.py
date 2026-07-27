import os
import uuid
import time
import logging
import json
from datetime import datetime, timezone

from flask import Flask, g, request as flask_request
from .config import config
from .health import bp as health_bp


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
logger = logging.getLogger("app")

app = Flask(__name__)
app.config["SERVICE_NAME"] = config.name


@app.after_request
def after_request(response):
    request_id = flask_request.headers.get("X-Request-ID", str(uuid.uuid4()))
    elapsed = time.time() - g.get("start_time", time.time())
    response.headers["X-Request-ID"] = request_id
    response.headers["Access-Control-Allow-Origin"] = "*"
    response.headers["Access-Control-Allow-Headers"] = "Content-Type, Authorization, X-Request-ID"
    response.headers["Access-Control-Expose-Headers"] = "X-Request-ID"
    logger.info("request", extra={
        "method": flask_request.method,
        "path": flask_request.path,
        "status": response.status_code,
        "latency_ms": int(elapsed * 1000),
        "request_id": request_id,
    })
    return response


@app.before_request
def before_request():
    g.start_time = time.time()


app.register_blueprint(health_bp)


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=config.port, debug=config.debug)
