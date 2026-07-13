from datetime import datetime, timezone

from flask import Blueprint, jsonify

bp = Blueprint("health", __name__)


@bp.route("/health")
def health():
    return jsonify({
        "status": "ok",
        "service": "{{project_name}}",
        "timestamp": datetime.now(timezone.utc).isoformat(),
    })
