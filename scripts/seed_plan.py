import sqlite3
import json
import uuid
import datetime

db_path = 'var/aos-cp.sqlite3'
manifest_path = 'manifests/qwen7b-4bit-mlx.yaml'
manifest_hash = 'c5a79aeb2e1ee710296a9dd7f810cdbb1c030c0294d6ac5436e706bc4be71985'

with open(manifest_path, 'r') as f:
    manifest_body = f.read()

conn = sqlite3.connect(db_path)
cursor = conn.cursor()

# 1. Insert Manifest
try:
    cursor.execute(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json, created_at) VALUES (?, ?, ?, ?, ?)",
        (str(uuid.uuid4()), 'default', manifest_hash, manifest_body, datetime.datetime.now().isoformat())
    )
except Exception as e:
    print(f"Error inserting manifest: {e}")

# 2. Insert Plan
plan_id = 'dev'
plan_id_b3 = 'dev-plan-hash'
kernel_hashes = json.dumps({"router": "default", "executor": "default"})

try:
    cursor.execute(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        (plan_id, 'default', plan_id_b3, manifest_hash, kernel_hashes, datetime.datetime.now().isoformat())
    )
except Exception as e:
    print(f"Error inserting plan: {e}")

conn.commit()
conn.close()
print("Seeding complete.")
