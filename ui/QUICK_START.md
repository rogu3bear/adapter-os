# Quick Start Guide - New Features

## Overview

Three new features have been added to AdapterOS Control Plane:

1. **IT Admin Dashboard** - System administration console
2. **User Reports** - Activity and metrics reporting
3. **Single-File Trainer** - Train adapters from a single file

---

## 1. IT Admin Dashboard

### Access
- Navigate to **Administration > IT Admin** in the sidebar
- Only available to users with **Admin** role

### What You Can Do

**Monitor System Health**
- View system status, CPU, memory, and disk usage
- Track active tenants and nodes
- See loaded models

**Manage Alerts**
- View critical system alerts
- Monitor alert history
- Quick access to alert management

**Tenant Administration**
- View all tenants and their status
- Quick tenant overview
- Access tenant management tools

**System Actions**
- Manage users
- Configure nodes
- Export system logs
- Adjust security settings

### Quick Actions
```
1. Click "IT Admin" in sidebar
2. Review system health at the top
3. Check for any critical alerts (red banner)
4. Review tenant and adapter statistics
5. Use action buttons for management tasks
```

---

## 2. User Reports Page

### Access
- Navigate to **Tools > Reports & Activity** in the sidebar
- Available to all authenticated users

### What You Can Do

**View Key Metrics**
- Active adapters count
- Training job statistics
- System latency (P95)
- Throughput (tokens/second)

**Track Training Jobs**
- See recent training jobs
- Monitor job status (completed/failed/running)
- View progress bars for active training

**Monitor Activity**
- Recent inference events
- Training activity
- System events
- Timestamped activity log

**Export Data**
- Export training history
- Export activity logs
- Export metrics summary

### Quick Actions
```
1. Click "Reports & Activity" in sidebar
2. View metrics at a glance
3. Scroll to see recent training jobs
4. Review activity feed
5. Use export buttons to download data
```

---

## 3. Single-File Adapter Trainer

### Access
- Navigate to **Tools > Single-File Trainer** in the sidebar
- Available to all authenticated users

### Step-by-Step Guide

#### Step 1: Upload Your File

```
1. Click on the upload area OR drag and drop your file
2. Supported formats: .txt, .json, .py, .js, .ts, .md
3. Max file size: 10MB
4. Preview your file content
5. Click "Continue to Configuration"
```

**Example Files**:
- Python code file: `my_functions.py`
- Training data: `training_examples.json`
- Documentation: `api_docs.md`

#### Step 2: Configure Training

```
1. Enter adapter name (auto-generated from filename)
2. Adjust training parameters:
   - LoRA Rank (8 recommended)
   - Alpha (16 recommended)
   - Epochs (3 for quick test, 10+ for production)
   - Batch Size (4 is good default)
   - Learning Rate (0.0003 default)
3. Click "Start Training"
```

**Parameter Guide**:
- **Lower Rank** = Faster, less capacity
- **Higher Rank** = Slower, more capacity
- **More Epochs** = Better learning, longer time
- **Larger Batch** = Faster, more memory

#### Step 3: Wait for Training

```
1. Watch the progress bar
2. Monitor current epoch
3. See training loss decrease
4. Training takes 2-15 minutes typically
```

The system polls automatically every 2 seconds for updates.

#### Step 4: Test & Download

```
1. Enter a test prompt
2. Click "Test Inference"
3. Review the model's response
4. Test multiple prompts if desired
5. Click "Download Adapter" to save .aos file
6. Click "Train Another Adapter" to start over
```

**Testing Tips**:
- Test with prompts similar to your training data
- Try edge cases to see how the adapter responds
- Check latency metrics in the response

### Complete Example

```bash
# Example: Train a Python code assistant

# Step 1: Upload
File: python_helpers.py (contains utility functions)

# Step 2: Configure
Adapter Name: python_helper_adapter
LoRA Rank: 8
Alpha: 16
Epochs: 5
Batch Size: 4
Learning Rate: 0.0003

# Step 3: Training
Progress: 0% -> 100% (approx 5 minutes)
Loss: 2.451 -> 0.892

# Step 4: Test
Prompt: "Create a function to calculate fibonacci"
Response: "def fibonacci(n): ..."

# Step 5: Download
File: python_helper_adapter.aos (ready to deploy)
```

---

## Navigation Quick Reference

```
📁 Workflow
  ├── Getting Started
  └── Dashboard

📁 ML Lifecycle
  ├── Train
  ├── Test & Validate
  ├── Compare Baselines
  ├── Promote
  └── Deploy & Manage

📁 Operations
  ├── Routing Inspector
  ├── Inference Playground
  └── System Health

📁 Security & Compliance
  ├── Policies
  ├── Telemetry
  ├── Replay & Verify
  └── Audit Trails

📁 Tools (NEW!)
  ├── 📤 Single-File Trainer
  └── 📊 Reports & Activity

🔧 Administration (NEW! - Admin only)
  └── ⚙️ IT Admin
```

---

## Tips & Best Practices

### For IT Admins
- Check the IT Admin dashboard daily
- Monitor critical alerts immediately
- Review resource usage trends
- Keep adapters memory usage under control
- Export logs regularly for compliance

### For All Users
- Use Reports page to track your activity
- Monitor training job success rates
- Check system metrics before starting heavy jobs
- Export reports for your records

### For Adapter Training
- Start with small files (< 1MB) for testing
- Use 3-5 epochs for initial testing
- Increase epochs for production adapters
- Test thoroughly before deploying
- Save your .aos files in a safe location
- Use descriptive adapter names

---

## Troubleshooting

### Cannot Access IT Admin Dashboard
- **Issue**: Page redirects to dashboard
- **Solution**: Contact your administrator to grant Admin role

### Training Fails Immediately
- **Issue**: Training job shows "failed" status
- **Solution**: 
  1. Check file format is supported
  2. Ensure file size is under 10MB
  3. Verify backend training service is running
  4. Check server logs for details

### No Data in Reports
- **Issue**: Reports page shows "No data"
- **Solution**:
  1. Start some training jobs
  2. Run some inference requests
  3. Wait a few minutes for data to populate
  4. Refresh the page

### Download Button Not Working
- **Issue**: Cannot download .aos file
- **Solution**:
  1. Ensure training completed successfully
  2. Check browser console for errors
  3. Verify artifact path exists in training job
  4. Contact support if issue persists

---

## Support

For additional help:
1. Check the comprehensive [Feature Overview](./FEATURE_OVERVIEW.md)
2. Review server logs at `/var/log/adapteros/`
3. Contact your system administrator
4. Open an issue on GitHub

---

## Next Steps

After completing this guide:

1. **Admins**: Set up monitoring alerts in the IT Admin dashboard
2. **Users**: Train your first adapter using the Single-File Trainer
3. **Everyone**: Bookmark the Reports page to track your activity
4. **Advanced**: Explore the API to automate your workflows

Happy training! 🚀

