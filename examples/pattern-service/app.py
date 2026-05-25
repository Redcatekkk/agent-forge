from fastapi import FastAPI

app = FastAPI()


@app.patch("/users/{user_id}")
def update_user(user_id: str, active: bool):
    """Update whether a user can sign in."""
    return {"id": user_id, "active": active}
