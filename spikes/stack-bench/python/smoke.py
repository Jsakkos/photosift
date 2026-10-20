import os, sys, json
os.environ["CACHE_DIR"] = sys.argv[2]
from fastapi.testclient import TestClient
import server
c = TestClient(server.app)
r = c.post("/api/ingest", json={"src": sys.argv[1]}); print(json.dumps(r.json(), indent=1))
for d in ("cpu", "cuda"):
    r = c.post("/api/enrich", json={"device": d}); print(json.dumps(r.json(), indent=1))
print(c.get("/api/photos").json())
print(c.get("/api/faces").json())
