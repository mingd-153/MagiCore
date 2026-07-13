import uvicorn
from fastapi import FastAPI
from src.config import config
from src.routes.health import router as health_router
from src.services.status import Status

app = FastAPI(title=config.name)
app.include_router(health_router)


@app.get("/")
async def root():
    return {
        "service": config.name,
        "framework": config.framework,
        "message": "{{project_name}} backend scaffold ready",
    }


@app.get("/status")
async def status():
    return Status().dict()


if __name__ == "__main__":
    uvicorn.run("main:app", host="0.0.0.0", port=config.port, reload=config.debug)
