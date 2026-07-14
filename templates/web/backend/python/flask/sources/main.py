import os
from flask import Flask
from .config import config
from .health import bp as health_bp

app = Flask(__name__)
app.config["SERVICE_NAME"] = config.name
app.register_blueprint(health_bp)

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=config.port, debug=config.debug)
