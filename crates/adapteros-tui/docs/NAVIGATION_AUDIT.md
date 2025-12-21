# adapterOS TUI Navigation & Routing Audit

## 📊 Canonical Route Architecture

### Current State Machine

```
┌─────────────┐
│   SCREENS   │ (6 total)
├─────────────┤
│ Dashboard   │ ← Entry point
│ Services    │
│ Logs        │
│ Metrics     │
│ Config      │
│ Help        │
└─────────────┘
     ↕
┌─────────────┐
│    MODES    │ (5 total)
├─────────────┤
│ Normal      │ ← Default
│ ServiceSelect│
│ LogView     │
│ ConfigEdit  │
│ Confirmation│
└─────────────┘
```

## 🔴 Issues Found

### 1. **Navigation Inconsistencies**

#### Screen Cycling is Circular but Unintuitive
```rust
// Current: Dashboard → Services → Logs → Metrics → Config → Help → Dashboard
// Problem: Left/Right don't align with visual expectations
pub fn on_left(&mut self) {
    Screen::Dashboard => Screen::Help,  // ❌ Wraps backwards
}
```
**Issue**: User expects left to go to previous tab, not wrap around to Help.

#### Menu Item Hardcoded Limit
```rust
if self.selected_menu_item < 6 {  // ❌ Magic number
    self.selected_menu_item += 1;
}
```
**Issue**: Adding menu items requires updating magic numbers in multiple places.

### 2. **State Management Problems**

#### Mode Transitions Not Properly Guarded
```rust
Mode::ServiceSelect => {
    self.boot_single_service(self.selected_service).await?;
    self.current_mode = Mode::Normal;  // ❌ Always returns to Normal
}
```
**Issue**: No way to cancel service selection or validate before action.

#### Unused Modes
- `LogView` - Defined but never set
- `ConfigEdit` - Defined but never set
**Issue**: Dead code that suggests incomplete features.

### 3. **Event Handling Issues**

#### Character Shortcuts Override Navigation
```rust
pub async fn on_char(&mut self, c: char) -> Result<()> {
    match c {
        's' => self.current_screen = Screen::Services,  // ❌ No mode check
```
**Issue**: Typing 's' while in ConfigEdit would jump to Services screen.

#### Quit Logic Always Returns True
```rust
pub fn should_quit(&self) -> bool {
    true  // ❌ No state cleanup or confirmation
}
```
**Issue**: 'q' instantly quits without saving state or confirming.

### 4. **Menu Navigation Problems**

#### Enter Key Behavior Inconsistent
```rust
match self.selected_menu_item {
    0 => self.boot_all_services().await?,     // Action
    1 => self.current_mode = Mode::ServiceSelect,  // Mode change
    3 => self.current_screen = Screen::Metrics,    // Screen change
```
**Issue**: Same key does different types of actions with no visual cues.

#### No Visual Feedback for Actions
```rust
self.confirmation_message = Some("Booting all services...".to_string());
// But services just set to "Starting" with no actual progress
```
**Issue**: User can't tell if action succeeded or is still running.

## ✅ Canonical Navigation Flow (Recommended)

### 1. **Screen Navigation**
```
Primary Navigation (Tab/Shift+Tab):
┌──────────┬──────────┬──────────┬──────────┬──────────┬──────────┐
│Dashboard │ Services │   Logs   │ Metrics  │  Config  │   Help   │
└──────────┴──────────┴──────────┴──────────┴──────────┴──────────┘
     ↑                                                          ↑
     └──────────────────── Tab cycles forward ─────────────────┘
     └────────────────── Shift+Tab cycles back ────────────────┘
```

### 2. **Mode Hierarchy**
```
Normal (base)
    ├─> ServiceSelect (Enter from menu)
    │       └─> Returns to Normal on Esc/Action
    ├─> LogView (automatic when viewing logs)
    │       └─> Returns to Normal on Esc
    ├─> ConfigEdit (Enter on config field)
    │       └─> Returns to Normal on Esc/Save
    └─> Confirmation (modal overlay)
            └─> Returns to previous mode
```

### 3. **Key Binding Priority**
```rust
// Canonical order:
1. Global exits (Ctrl+C, Ctrl+Q)
2. Mode-specific handlers
3. Navigation (arrows, tab)
4. Screen shortcuts (only in Normal mode)
5. Character input (only in edit modes)
```

## 🛠️ Required Fixes

### Fix 1: Proper Mode State Machine
```rust
pub enum Mode {
    Normal,
    ServiceSelect { from_screen: Screen },  // Track where we came from
    LogView { filter: Option<String> },     // Store view state
    ConfigEdit { field: ConfigField },      // Track what's being edited
    Confirmation { action: PendingAction }, // Store what needs confirming
}
```

### Fix 2: Navigation Guards
```rust
pub fn can_navigate(&self) -> bool {
    matches!(self.current_mode, Mode::Normal | Mode::ServiceSelect { .. })
}

pub fn on_left(&mut self) {
    if !self.can_navigate() { return; }
    // ... navigation logic
}
```

### Fix 3: Proper Screen Ordering
```rust
const SCREEN_ORDER: &[Screen] = &[
    Screen::Dashboard,
    Screen::Services,
    Screen::Logs,
    Screen::Metrics,
    Screen::Config,
    Screen::Help,
];

pub fn next_screen(&mut self) {
    let current_idx = SCREEN_ORDER.iter()
        .position(|&s| s == self.current_screen)
        .unwrap_or(0);
    let next_idx = (current_idx + 1) % SCREEN_ORDER.len();
    self.current_screen = SCREEN_ORDER[next_idx];
}
```

### Fix 4: Action Queue System
```rust
pub struct App {
    // ...
    pending_actions: VecDeque<Action>,
    action_results: HashMap<ActionId, ActionResult>,
}

pub enum Action {
    BootService(String),
    StopService(String),
    SaveConfig(ConfigUpdate),
}
```

## 📋 Navigation Rules

### Dashboard Screen
- **Up/Down**: Navigate menu items
- **Enter**: Execute menu action
- **Tab**: Go to Services
- **Esc**: Clear any messages

### Services Screen
- **Up/Down**: Select service
- **Enter**: Enter service action mode
- **s/S**: Start service
- **r/R**: Restart service
- **d/D**: Stop service
- **Tab**: Go to Logs
- **Esc**: Back to Dashboard

### Logs Screen
- **Up/Down**: Scroll logs
- **/**: Enter search mode
- **f/F**: Open filter menu
- **c/C**: Clear filters
- **Tab**: Go to Metrics
- **Esc**: Back to Dashboard

### Metrics Screen
- **Up/Down**: Scroll metrics
- **r/R**: Refresh data
- **Tab**: Go to Config
- **Esc**: Back to Dashboard

### Config Screen
- **Up/Down**: Navigate fields
- **Enter**: Edit field
- **s/S**: Save changes
- **r/R**: Reset to defaults
- **Tab**: Go to Help
- **Esc**: Back to Dashboard (with unsaved check)

### Help Screen
- **Up/Down**: Scroll help
- **Tab**: Go to Dashboard
- **Esc**: Back to Dashboard

## 🎯 Implementation Priority

1. **High Priority**
   - Fix mode state machine
   - Add navigation guards
   - Fix character input handling

2. **Medium Priority**
   - Implement action queue
   - Add progress indicators
   - Fix screen ordering

3. **Low Priority**
   - Add animation transitions
   - Implement undo/redo
   - Add command palette

## 📊 Metrics to Track

- Navigation latency (should be < 16ms)
- Mode transition success rate
- User input drop rate
- Screen render time