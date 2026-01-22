---
description: Create, train, and manage LoRA adapters
---

# Adapter Workflow

## List Adapters

// turbo

```bash
./aosctl adapter list
```

## Train from Codebase

```bash
./aosctl adapter train-from-code \
  --path ./my-project \
  --name my-adapter \
  --description "Trained on my code"
```

## Train from Dataset

```bash
./aosctl datasets upload --path ./data.ndjson --name my-dataset
./aosctl train --dataset my-dataset --name my-adapter --epochs 3
```

## Register Adapter

```bash
./aosctl adapter register --path var/adapters/my-adapter --name my-adapter
```

## Swap Active Adapter

```bash
./aosctl adapter swap --to <adapter-id>
```

## Verify & Repair

```bash
./aosctl adapter verify <adapter-id>
./aosctl adapter repair-hashes <adapter-id>
```

## Get Info

```bash
./aosctl adapter info <adapter-id>
```
