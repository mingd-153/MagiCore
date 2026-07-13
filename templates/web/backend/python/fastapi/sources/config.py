import os


class Config:
    name: str = "{{project_name}}"
    framework: str = "fastapi"
    port: int = int(os.getenv("PORT", "3000"))
    debug: bool = os.getenv("DEBUG", "").lower() in ("1", "true", "yes")


config = Config()
