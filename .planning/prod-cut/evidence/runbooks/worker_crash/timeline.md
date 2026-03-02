# Timeline (UTC)

- 2026-03-02T07:33:55Z: Drill started (`pgrep -fl "aos-worker|adapteros-worker|training-worker"`) and no running worker process was found.
- 2026-03-02T07:33:55Z: `./start worker` invoked.
- 2026-03-02T07:33:57Z: Startup failed with `Worker process died during startup` and `start_worker_exit=1`.
- 2026-03-02T07:33:57Z: `tail -n 120 var/logs/worker.log` captured tokenizer validation fatal (`Tokenizer vocab_size 248070 exceeds manifest/base config 152064`).
- 2026-03-02T07:33:57Z: Control plane status/readiness were collected (`/v1/status`, `/readyz`) showing service ready with non-critical worker degradation.
