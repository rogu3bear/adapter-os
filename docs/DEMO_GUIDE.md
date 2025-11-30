# AdapterOS Demo Guide

**Your First Adapter in 4 Easy Steps**

This guide walks you through the complete AdapterOS demo experience - from logging in to seeing your custom-trained adapter in action. No technical knowledge required!

---

## Overview: What You'll Do

| Step | What | Time |
|------|------|------|
| 1 | **Login** - Get access without credentials | ~5 seconds |
| 2 | **Train** - Create an adapter from your code | ~2-5 minutes |
| 3 | **Load** - Prepare your adapter for use | ~10 seconds |
| 4 | **Compare** - See the difference your adapter makes | ~1 minute |

**Total time:** About 5-10 minutes

---

## Step 1: Dev Bypass Login

### Getting Started Without Credentials

When you first open the app, you'll see a login screen. For the demo, you don't need to create an account or remember any passwords!

**What you'll see:**

```
+----------------------------------+
|         AdapterOS Login          |
|                                  |
|  [ Username field ]              |
|  [ Email field ]                 |
|  [ Password field ]              |
|                                  |
|  [ Secure Login button ]         |
|                                  |
|  --------------------------------|
|                                  |
|  [ Dev Bypass (No Auth) button ] |
|                                  |
+----------------------------------+
```

**What to do:**

1. Look for the button that says **"Dev Bypass (No Auth Required)"** at the bottom of the login form
2. Click it once
3. Wait a moment while the system activates

**What happens:**

- The system creates a temporary admin session for you
- You get full access to all features
- No credentials are stored or required

**Expected result:**

- The screen changes to show the **Dashboard**
- You'll see system health indicators and quick stats
- A welcome message may appear

**Why this exists:**

The Dev Bypass lets you explore the full system without setting up authentication. It's perfect for demos, testing, and getting familiar with the platform.

---

## Step 2: Single-File Trainer

### Train on Your Code Sample

Now let's create your first adapter! The Single-File Trainer is the easiest way to get started - just upload a file with your code or text, and the system handles the rest.

**How to get there:**

From the Dashboard, click **"Trainer"** in the left sidebar, or navigate to `/trainer` in your browser.

**What you'll see:**

A 4-step progress wizard:

```
[ Upload File ] --> [ Configure ] --> [ Training ] --> [ Test & Download ]
     (1)               (2)              (3)               (4)
```

### Step 2a: Upload Your File

**What you'll see:**

```
+----------------------------------------+
|  Upload Training Data                  |
|                                        |
|  +----------------------------------+  |
|  |                                  |  |
|  |     [ File icon ]                |  |
|  |                                  |  |
|  |     Click to upload file         |  |
|  |                                  |  |
|  |     Supports .txt, .json, .py,   |  |
|  |     .js, .ts, .md (max 10MB)     |  |
|  |                                  |  |
|  +----------------------------------+  |
|                                        |
+----------------------------------------+
```

**What to do:**

1. Click the dashed box area
2. Select a code file from your computer
   - Good examples: a Python file, JavaScript module, or text document
   - Any file you want the adapter to "learn" from
3. Wait for the preview to appear

**What happens:**

- Your file is read and displayed in a preview
- A name is automatically generated based on your filename
- The system shows the file size

**Expected result:**

- You see your file content in the preview area
- The **"Continue to Configuration"** button appears
- An adapter name is pre-filled (like `myfile_adapter`)

**Tips for good training files:**

- Use code you want the adapter to understand well
- Documentation or README files work great
- Even a few dozen lines can produce useful results

### Step 2b: Configure Training

**What you'll see:**

```
+----------------------------------------+
|  Training Configuration                |
|                                        |
|  Adapter Name: [ my_code_adapter    ]  |
|                                        |
|  +-----------------+-----------------+ |
|  | LoRA Rank: [8 ] | Alpha: [16    ] | |
|  +-----------------+-----------------+ |
|  | Epochs: [3    ] | Batch Size: [4]|  |
|  +-----------------+-----------------+ |
|  | Learning Rate: [0.0003         ] |  |
|  +----------------------------------+  |
|                                        |
|  [ Back ]        [ Start Training ]    |
+----------------------------------------+
```

**What to do:**

1. Review the adapter name (change if you want)
2. **For the demo, leave the default settings** - they're optimized for quick results
3. Click **"Start Training"**

**What the settings mean (optional reading):**

- **Adapter Name:** What your adapter will be called
- **LoRA Rank:** How much the adapter can modify (8 is a good balance)
- **Alpha:** Strength of changes (16 works well)
- **Epochs:** How many times to process the data (3 is usually enough)
- **Batch Size:** How much to process at once (4 for stability)
- **Learning Rate:** How quickly to learn (0.0003 is safe)

### Step 2c: Watch Training Progress

**What you'll see:**

```
+----------------------------------------+
|  Training in Progress                  |
|                                        |
|  [===========............] 45%         |
|                                        |
|  Epoch 2 of 3                          |
|  Estimated time remaining: 2-4 min     |
|                                        |
|  +----------------+------------------+ |
|  | Current Epoch  | Training Loss    | |
|  |     2 / 3      |    0.0234        | |
|  +----------------+------------------+ |
|                                        |
+----------------------------------------+
```

**What to do:**

1. Wait! The system is learning from your file
2. Watch the progress bar fill up
3. See the training loss decrease (lower is better)

**What happens:**

- The system reads through your file multiple times (epochs)
- It learns patterns and style from your content
- A progress indicator shows you how far along it is

**Expected result:**

- Training completes in 2-5 minutes (depending on file size)
- The screen automatically moves to "Test & Download"
- You see a success message

**If something goes wrong:**

- An error message will appear with a **Retry** option
- You can go back and try with different settings
- Contact support if errors persist

---

## Step 3: Load Adapter

### Prepare Your Trained Adapter

After training completes, you need to "load" your adapter so the system can use it. This happens on the Adapters page.

**How to get there:**

Click **"Adapters"** in the left sidebar, or navigate to `/adapters`

**What you'll see:**

```
+----------------------------------------------------------+
|  Adapters                                    [ Train New ] |
|                                                           |
|  +--------+--------+--------+--------+                    |
|  | Total  | Loaded | Pinned | Memory |                    |
|  |   5    |   2    |   1    | 256 MB |                    |
|  +--------+--------+--------+--------+                    |
|                                                           |
|  +-------------------------------------------------------+|
|  | Name           | Status    | Actions                  ||
|  |----------------|-----------|--------------------------|
|  | my_code_adapter| Unloaded  | [ Load ] [ Delete ]      ||
|  | rust-expert    | Warm      | [ Unload ] [Pin] [More]  ||
|  | python-helper  | Hot       | [ Unload ] [Pin] [More]  ||
|  +-------------------------------------------------------+|
+----------------------------------------------------------+
```

**What to do:**

1. Find your newly trained adapter in the list (it will be named what you chose earlier)
2. Look at the **Status** column - it should say "Unloaded" or "Cold"
3. Click the **[ Load ]** button next to your adapter

**What happens:**

- The adapter is loaded into memory
- Status changes: Unloaded -> Cold -> Warm (or Hot)
- The adapter is now ready for use in inference

**Expected result:**

- Status changes to **"Warm"** or **"Hot"**
- The Loaded count at the top increases
- Your adapter is now ready to use!

**Understanding adapter states:**

| State | Meaning |
|-------|---------|
| **Unloaded** | Not in memory, can't be used yet |
| **Cold** | Being loaded |
| **Warm** | Ready to use |
| **Hot** | Frequently used, prioritized |
| **Resident** | Pinned, won't be removed |

---

## Step 4: Compare with Inference

### See the Difference Your Adapter Makes

This is the exciting part - you'll see how your adapter changes the AI's responses compared to the base model!

**How to get there:**

Click **"Inference"** in the left sidebar, or navigate to `/inference`

**What you'll see:**

```
+----------------------------------------------------------+
|  Inference Playground                                     |
|                                                           |
|  Adapter: [ Select adapter... v ]                         |
|                                                           |
|  +-------------------------------------------------------+|
|  |                                                       ||
|  |  Enter your prompt here...                            ||
|  |                                                       ||
|  +-------------------------------------------------------+|
|                                                           |
|  [ Generate ]    [ Compare Mode ]    [ Streaming ]        |
|                                                           |
+----------------------------------------------------------+
```

### Comparison Mode: Before and After

**What to do:**

1. Click **[ Compare Mode ]** to enable side-by-side comparison
2. Select your adapter from the dropdown
3. Enter a prompt related to your training data
   - Example: If you trained on Python code, ask about Python
   - If you trained on documentation, ask about those topics
4. Click **[ Generate ]**

**What you'll see in Compare Mode:**

```
+---------------------------+---------------------------+
|     Base Model Response   |    With Your Adapter      |
+---------------------------+---------------------------+
|                           |                           |
|  Generic response about   |  Response tailored to     |
|  the topic...             |  your training data...    |
|                           |                           |
|  May be accurate but      |  Includes patterns and    |
|  lacks specific style     |  style from your file     |
|                           |                           |
+---------------------------+---------------------------+
```

**What happens:**

- The system runs your prompt twice:
  1. Once with just the base model
  2. Once with your adapter applied
- Both responses appear side-by-side
- You can see exactly what your adapter changed

**Expected result:**

- Two responses appear simultaneously
- The "With Your Adapter" side shows influence from your training data
- The response style, terminology, or focus reflects your training file

**Tips for good comparisons:**

- Ask questions related to your training content
- Try prompts that might use terminology from your file
- Compare code style if you trained on code
- Notice differences in tone, structure, or details

---

## What You Just Accomplished

Congratulations! You've completed the core AdapterOS workflow:

1. **Logged in** using the frictionless dev bypass
2. **Trained** a custom LoRA adapter from your own file
3. **Loaded** the adapter into the system
4. **Compared** responses to see your adapter's impact

### What's Next?

Now that you understand the basics, you can:

- **Train more adapters** from different files or datasets
- **Stack multiple adapters** for combined effects
- **Monitor performance** on the Metrics page
- **Fine-tune settings** for better results
- **Export adapters** to share with others

---

## Quick Troubleshooting

| Problem | Solution |
|---------|----------|
| "Dev Bypass" button not visible | Make sure you're running in development mode |
| Training takes too long | Try a smaller file or fewer epochs |
| Adapter won't load | Check system memory on the Dashboard |
| No difference in Compare Mode | Try prompts more related to your training data |
| "Permission denied" error | Verify you're using Dev Bypass login |

---

## Need Help?

- **Dashboard** shows system health and alerts
- **Help Center** (`/help`) has detailed documentation
- Check the **Metrics** page for performance insights
- View **Training** page for job history and logs

---

**Built for Apple Silicon** | AdapterOS Demo Guide

