import os
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent.parent

SECRET_KEY = os.environ.get("DJANGO_SECRET_KEY", "change-me-in-production")
DEBUG = os.environ.get("DEBUG", "").lower() in ("1", "true", "yes")
ALLOWED_HOSTS = ["*"]

MIDDLEWARE = [
    "src.middleware.RequestLoggingMiddleware",
]

INSTALLED_APPS = [
    "django.contrib.contenttypes",
]

ROOT_URLCONF = "src.urls"

WSGI_APPLICATION = "src.wsgi.application"

LANGUAGE_CODE = "en-us"
TIME_ZONE = "UTC"
USE_TZ = True

SERVICE_NAME = "{{project_name}}"
SERVICE_FRAMEWORK = "django"
