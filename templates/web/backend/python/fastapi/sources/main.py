import os
from fastapi import FastAPI
import uvicorn
from .config import config
from .health import router as health_router

app = FastAPI(title=config.name)
app.include_router(health_router)

if __name__ == "__main__":
    uvicorn.run("src.main:app", host="0.0.0.0", port=config.port, reload=True)
