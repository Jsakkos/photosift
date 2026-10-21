"""Benchmark harness: launches one server implementation, drives it, records
startup, ingest, enrichment (CPU + CUDA), HTTP preview latency and peak RSS.

usage: python bench/run.py <rust|python> <src_dir> <label> [--serve]
"""
import json
import os
import shutil
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

import httpx
import psutil

ROOT = Path(__file__).resolve().parent.parent
CUDA_DLL_DIR = "C:/Github/photosift/src-tauri/target/debug"
IMPLS = {
    "rust": ([str(ROOT / "rust/target/release/stack-bench-rs.exe")], 8701),
    "python": ([str(ROOT / "python/.venv/Scripts/python.exe"), str(ROOT / "python/server.py")], 8702),
}


class RssPoller(threading.Thread):
    def __init__(self, pid):
        super().__init__(daemon=True)
        self.proc = psutil.Process(pid)
        self.peak = 0
        self.stop = False

    def rss(self):
        procs = [self.proc] + self.proc.children(recursive=True)
        return sum(p.memory_info().rss for p in procs if p.is_running())

    def run(self):
        while not self.stop:
            try:
                self.peak = max(self.peak, self.rss())
            except psutil.Error:
                pass
            time.sleep(0.05)

    def phase(self):
        """Returns peak since last call (MB) and resets."""
        p, self.peak = self.peak, 0
        return round(p / 2**20, 1)


def main():
    impl, src, label = sys.argv[1], sys.argv[2], sys.argv[3]
    serve = "--serve" in sys.argv
    cmd, port = IMPLS[impl]
    cache = ROOT / f"cache-{impl}"
    if "--serve-only" in sys.argv:
        # Reuse the cache from the last batch run; just host it for browser tests.
        env = dict(os.environ, CACHE_DIR=str(cache), PORT=str(port),
                   PATH=CUDA_DLL_DIR + os.pathsep + os.environ["PATH"])
        subprocess.run(cmd, env=env)
        return
    shutil.rmtree(cache, ignore_errors=True)
    env = dict(os.environ, CACHE_DIR=str(cache), PORT=str(port),
               PATH=CUDA_DLL_DIR + os.pathsep + os.environ["PATH"])
    base = f"http://127.0.0.1:{port}"
    out = {"impl": impl, "label": label, "src": src}

    t0 = time.perf_counter()
    proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=open(ROOT / f"bench/{impl}.stderr.log", "w"))
    client = httpx.Client(base_url=base, timeout=None)
    while True:
        try:
            if client.get("/api/health").status_code == 200:
                break
        except httpx.TransportError:
            time.sleep(0.01)
        if proc.poll() is not None:
            sys.exit(f"{impl} server exited during startup")
    out["startup_ms"] = round((time.perf_counter() - t0) * 1000, 1)
    poller = RssPoller(proc.pid)
    poller.start()
    time.sleep(0.3)
    out["idle_rss_mb"] = poller.phase()

    try:
        r = client.post("/api/ingest", json={"src": src}).raise_for_status().json()
        r["peak_rss_mb"] = poller.phase()
        out["ingest"] = r
        for device in ("cpu", "cuda"):
            r = client.post("/api/enrich", json={"device": device}).raise_for_status().json()
            r["peak_rss_mb"] = poller.phase()
            out[f"enrich_{device}"] = r

        t = time.perf_counter()
        photos = client.get("/api/photos").raise_for_status().json()
        out["photos_json_ms"] = round((time.perf_counter() - t) * 1000, 1)

        # Server-side preview latency without a browser: sequential GETs.
        lat = []
        for p in photos[:200]:
            t = time.perf_counter()
            client.get(f"/preview/{p['id']}").raise_for_status()
            lat.append((time.perf_counter() - t) * 1000)
        q = statistics.quantiles(lat, n=100)
        out["http_preview_ms"] = {"p50": round(q[49], 2), "p90": round(q[89], 2), "p99": round(q[98], 2)}

        # Burst: 16 concurrent preview GETs, like a prefetching UI.
        import concurrent.futures as cf
        with cf.ThreadPoolExecutor(16) as ex:
            t = time.perf_counter()
            list(ex.map(lambda p: httpx.get(f"{base}/preview/{p['id']}").raise_for_status(), photos[:160]))
            out["http_preview_burst160_ms"] = round((time.perf_counter() - t) * 1000, 1)
        out["serve_peak_rss_mb"] = poller.phase()
        out["settled_rss_mb"] = round(poller.rss() / 2**20, 1)
    finally:
        res = ROOT / "bench/results"
        res.mkdir(exist_ok=True)
        (res / f"{impl}-{label}.json").write_text(json.dumps(out, indent=1))
        print(json.dumps(out, indent=1))
        if serve:
            print(f"serving {impl} on {base} (pid {proc.pid})", flush=True)
            proc.wait()
        else:
            proc.terminate()


if __name__ == "__main__":
    main()
