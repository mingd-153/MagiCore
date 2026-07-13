from flask import Flask, jsonify
from src.config import config
from src.routes.health import bp as health_bp
from src.services.status import Status

app = Flask(__name__)
app.register_blueprint(health_bp)


@app.route("/")
def root():
    return jsonify({
        "service": config.name,
        "framework": config.framework,
        "message": "{{project_name}} backend scaffold ready",
    })


@app.route("/status")
def status():
    return jsonify(Status().dict())


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=config.port, debug=config.debug)
