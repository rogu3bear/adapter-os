# Leptos UI Patterns

## Core Module: `crates/adapteros-ui/`

### Tech Stack
- Leptos 0.7 with CSR (Client-Side Rendering)
- WASM target (`wasm32-unknown-unknown`)
- Pure CSS (Liquid Glass design system)
- Trunk bundler

---

## Directory Structure

```
src/
├── api/client.rs      # ApiClient with all API methods
├── components/        # Reusable UI components
├── hooks/             # Custom hooks (use_api_resource, use_polling)
├── pages/             # Route pages (~25 pages)
├── signals/           # Global signals (auth, chat, modal, notifications)
├── sse.rs             # SSE event parsing
├── validation.rs      # Form validation
└── lib.rs             # App entry, providers, router
```

---

## API Client (`api/client.rs`)

### Core Struct
```rust
pub struct ApiClient {
    base_url: String,
    auth_token: RwSignal<Option<String>>,
}
```

### Key Methods
```rust
impl ApiClient {
    pub fn new() -> Self                    // Create client with auth token from storage
    pub fn with_base_url(url: &str) -> Self // Custom base URL
    pub fn is_authenticated(&self) -> bool
    pub fn set_token(&self, token: String)
    
    // HTTP methods
    pub async fn get<T>(&self, path: &str) -> ApiResult<T>
    pub async fn post<T, R>(&self, path: &str, body: &T) -> ApiResult<R>
    pub async fn put<T, R>(&self, path: &str, body: &T) -> ApiResult<R>
    pub async fn delete(&self, path: &str) -> ApiResult<()>
}
```

### CSRF Handling
```rust
fn csrf_token_from_cookie() -> Option<String>  // Reads from cookies
```

### API Methods (~100+)
Organized by domain: `list_adapters`, `get_training_job`, `create_stack`, `infer_stream_url`, etc.

---

## Hooks (`hooks/mod.rs`)

### `use_api_resource<T>` (Primary Data Fetching)
```rust
pub fn use_api_resource<T, F, Fut>(fetch: F) -> (ReadSignal<LoadingState<T>>, Callback<()>)
```
- Returns `(state, refetch)` tuple
- Automatic error reporting
- Version tracking prevents stale updates
- Defers to microtask to avoid RefCell re-entrancy

### `LoadingState<T>` Enum
```rust
pub enum LoadingState<T> {
    Idle,
    Loading,
    Loaded(T),
    Error(ApiError),
}
```

### Other Hooks
- `use_polling(interval_ms, fetch)` - Periodic data refresh
- `use_conditional_polling(should_poll, interval_ms, fetch)` - Conditional polling
- `use_navigate()` - Programmatic navigation
- `use_api()` - Get ApiClient instance

---

## Signals (`signals/`)

### Auth Signal (`signals/auth.rs`)

#### `AuthState` Enum
```rust
pub enum AuthState {
    Checking,
    Authenticated(User),
    Unauthenticated,
    Error(String),
}
```

#### `AuthAction` Struct
```rust
pub struct AuthAction {
    client: Arc<ApiClient>,
    state: RwSignal<AuthState>,
    attempt_id: RwSignal<u32>,
}
```

#### Methods
```rust
auth.login(&credentials).await
auth.logout().await
auth.check_auth().await
auth.current_attempt() -> u32
```

#### Context Provider
```rust
fn provide_auth_context()  // Call in App root
fn use_auth() -> AuthContext  // Access in components
```

### Other Signals
- `signals/chat.rs` - Chat state and messages
- `signals/modal.rs` - Modal dialog state
- `signals/notifications.rs` - Toast notifications
- `signals/refetch.rs` - Global refetch triggers
- `signals/search.rs` - Search state

---

## Components (`components/`)

### Available Components (~40)
| Component | Purpose |
|-----------|---------|
| `Button` | Primary button with variants |
| `Card` | Container with glass styling |
| `DataTable` | Data grid with sorting/pagination |
| `Dialog` | Modal dialog wrapper |
| `FormField` | Form input with validation |
| `Input` | Text input component |
| `Spinner` | Loading spinner |
| `Toast` | Toast notifications |
| `Tabs` | Tab navigation |
| `Table` | Basic table |
| `AsyncBoundary` | Loading/error boundary |

### Button Variants
```rust
pub enum ButtonVariant {
    Primary, Secondary, Danger, Ghost, Link
}
pub enum ButtonSize {
    Small, Medium, Large
}
```

### Async Boundaries
```rust
fn AsyncBoundary<T>(
    state: ReadSignal<LoadingState<T>>,
    children: impl Fn(T) -> impl IntoView,
) -> impl IntoView

fn AsyncBoundaryWithEmpty<T>(
    state: ReadSignal<LoadingState<T>>,
    empty_variant: EmptyStateVariant,
    children: impl Fn(T) -> impl IntoView,
) -> impl IntoView
```

### Empty State Variants
```rust
pub enum EmptyStateVariant {
    NoData, NoResults, Error, NotFound, Pending
}
```

---

## SSE Streaming (`sse.rs`)

### Event Types
```rust
pub enum InferenceEvent {
    Token(String),
    Done { trace_id: Option<String>, latency_ms: Option<u64>, ... },
    Ready,
    Other(String),
}
```

### Parsing
```rust
pub fn parse_sse_event_with_info(data: &str) -> ParsedSseEvent
```

### Streaming Chunks (OpenAI Compatible)
```rust
pub struct StreamingChunk {
    pub choices: Vec<StreamingChoice>,
}
pub struct StreamingChoice {
    pub delta: Delta,
    pub finish_reason: Option<String>,
}
```

---

## Form Validation (`validation.rs`)

### `ValidationRule` Enum
```rust
pub enum ValidationRule {
    Required,
    MinLength(usize),
    MaxLength(usize),
    Email,
    PositiveNumber,
    Custom(fn(&str) -> Option<String>),
}
```

### `FormErrors` Struct
```rust
pub struct FormErrors {
    errors: RwSignal<HashMap<String, String>>,
}

impl FormErrors {
    pub fn new() -> Self
    pub fn set(&self, field: &str, msg: &str)
    pub fn clear(&self, field: &str)
    pub fn get(&self, field: &str) -> Option<String>
    pub fn has_error(&self, field: &str) -> bool
    pub fn is_valid(&self) -> bool
}
```

### Built-in Rule Functions (`validation::rules`)
- `adapter_name(&str)` - Validate adapter names
- `email(&str)` - Email format
- `password(&str)` - Password requirements
- `positive_int(&str)` - Positive integer
- `learning_rate(&str)` - Valid LR range
- `description(&str)` - Description length

---

## Pages (`pages/`)

### Route Pages (~26)
`adapters`, `admin`, `agents`, `audit`, `chat`, `collections`, `dashboard`, `datasets`, `diff`, `documents`, `errors`, `flight_recorder`, `login`, `models`, `monitoring`, `not_found`, `policies`, `repositories`, `reviews`, `routing`, `safe`, `settings`, `stacks`, `style_audit`, `system`, `training`, `workers`

### Page Pattern
```rust
#[component]
pub fn MyPage() -> impl IntoView {
    // 1. Get resources
    let (data, refetch) = use_api_resource(|client| async move {
        client.list_items().await
    });

    // 2. Render with AsyncBoundary
    view! {
        <PageHeader title="My Page" />
        <AsyncBoundary state=data let:items>
            <DataTable rows=items />
        </AsyncBoundary>
    }
}
```

---

## App Entry (`lib.rs`)

### Providers
```rust
fn App() -> impl IntoView {
    view! {
        <ChatProvider>
            <NotificationsProvider>
                <SearchProvider>
                    <RefetchProvider>
                        <Router>
                            // Routes...
                        </Router>
                    </RefetchProvider>
                </SearchProvider>
            </NotificationsProvider>
        </ChatProvider>
    }
}
```

### Boot Sequence
```rust
pub fn mount() {
    set_dom_panic_hook();
    boot_log("Starting...");
    leptos::mount::mount_to_body(App);
}
```

---

## Common Patterns

### 1. Data Fetching Page
```rust
#[component]
pub fn ItemsPage() -> impl IntoView {
    let (items, refetch) = use_api_resource(|c| async move {
        c.list_items().await
    });

    view! {
        <PageHeader title="Items">
            <RefreshButton on_click=move |_| refetch.call(()) />
        </PageHeader>
        <AsyncBoundary state=items let:data>
            <ItemsList items=data />
        </AsyncBoundary>
    }
}
```

### 2. Form with Validation
```rust
#[component]
fn CreateForm() -> impl IntoView {
    let (name, set_name) = signal(String::new());
    let errors = use_form_errors();

    let on_submit = move |_| {
        if validate_field(&name.get(), &[ValidationRule::Required]).is_some() {
            errors.set("name", "Name is required");
            return;
        }
        // Submit...
    };

    view! {
        <FormField label="Name" error=errors.get("name")>
            <Input value=name on_input=move |v| set_name.set(v) />
        </FormField>
        <Button on_click=on_submit>"Create"</Button>
    }
}
```

### 3. Dialog with Confirmation
```rust
#[component]
fn DeleteButton(id: String) -> impl IntoView {
    let (show_dialog, set_show_dialog) = signal(false);

    view! {
        <Button variant=ButtonVariant::Danger on_click=move |_| set_show_dialog.set(true)>
            "Delete"
        </Button>
        <Show when=move || show_dialog.get()>
            <ConfirmationDialog
                title="Delete Item?"
                on_confirm=move || { /* delete logic */ }
                on_cancel=move || set_show_dialog.set(false)
            />
        </Show>
    }
}
```

---

## Build & Test

```bash
# WASM check
cargo check -p adapteros-ui --target wasm32-unknown-unknown

# Dev server
cd crates/adapteros-ui && trunk serve

# Production build
trunk build --release

# Unit tests (native)
cargo test -p adapteros-ui --lib
```
