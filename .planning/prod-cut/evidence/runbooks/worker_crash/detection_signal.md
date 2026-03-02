# Detection Signal

Primary detection signals observed during the live drill:

- `./start worker` returned non-zero with:
  - `Worker process died during startup. Check logs: /Users/star/Dev/adapter-os/var/logs/worker.log`
  - `start_worker_exit=1`
- Worker log contained fatal initialization evidence:
  - `Tokenizer validation failed ... Tokenizer vocab_size 248070 exceeds manifest/base config 152064`
  - `Worker exiting with error ... exit_code=1`
- `/v1/status` showed degraded worker posture:
  - `"non_critical_degraded":["workers","ui"]`
