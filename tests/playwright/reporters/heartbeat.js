import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..', '..');
const heartbeatPath = path.resolve(repoRoot, 'var/playwright/heartbeat.json');

function writeHeartbeat(payload) {
  fs.mkdirSync(path.dirname(heartbeatPath), { recursive: true });
  const tmpPath = `${heartbeatPath}.tmp`;
  const record = { ts: Date.now(), ...payload };
  fs.writeFileSync(tmpPath, JSON.stringify(record) + '\n');
  fs.renameSync(tmpPath, heartbeatPath);
}

export default class HeartbeatReporter {
  onBegin(config, suite) {
    writeHeartbeat({
      event: 'begin',
      testCount: suite.allTests().length,
      workers: config.workers,
    });
  }

  onTestBegin(test) {
    writeHeartbeat({
      event: 'test_begin',
      title: test.titlePath().join(' > '),
    });
  }

  onTestEnd(test, result) {
    writeHeartbeat({
      event: 'test_end',
      title: test.titlePath().join(' > '),
      status: result.status,
      durationMs: result.duration,
    });
  }

  onError(error) {
    writeHeartbeat({
      event: 'error',
      message: error?.message ?? String(error),
    });
  }

  onExit() {
    writeHeartbeat({ event: 'exit' });
  }
}

