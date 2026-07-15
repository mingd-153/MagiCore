import platform


class Status:
    def __init__(self):
        self.service = "{{project_name}}"
        self.version = "0.1.0"
        self.python_version = platform.python_version()

    def dict(self):
        return {
            "service": self.service,
            "version": self.version,
            "python_version": self.python_version,
        }
