use adapteros_tui::app::{App, Mode, Screen};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

#[tokio::test]
async fn test_fuzz_navigation_state() {
    // 1. Initialize App (this might fail if local environment issues, but DbClient degrades gracefully)
    let mut app = match App::new().await {
        Ok(app) => app,
        Err(e) => {
            eprintln!("Skipping fuzz test: Failed to initialize App: {}", e);
            return;
        }
    };

    // 2. Pre-populate some dummy data to allow navigation to occur
    app.adapters = vec![
        adapteros_tui::app::api::AdapterInfo {
            id: "adapter-1".to_string(),
            name: "Adapter 1".to_string(),
            version: "v1".to_string(),
            loaded: false,
            pinned: false,
            memory_mb: Some(100),
        },
        adapteros_tui::app::api::AdapterInfo {
            id: "adapter-2".to_string(),
            name: "Adapter 2".to_string(),
            version: "v1".to_string(),
            loaded: true,
            pinned: true,
            memory_mb: Some(200),
        },
    ];

    app.services = vec![
        adapteros_tui::app::types::ServiceStatus {
            name: "Service 1".to_string(),
            status: adapteros_tui::app::types::Status::Running,
            message: "OK".to_string(),
        },
        adapteros_tui::app::types::ServiceStatus {
            name: "Service 2".to_string(),
            status: adapteros_tui::app::types::Status::Stopped,
            message: "Stopped".to_string(),
        },
    ];

    // 3. Fuzz loop
    // Use a fixed seed for reproducibility
    let mut rng = StdRng::seed_from_u64(42);

    for i in 0..10000 {
        // Randomly switch screens sometimes
        if rng.gen_bool(0.1) {
            match rng.gen_range(0..9) {
                0 => app.current_screen = Screen::Dashboard,
                1 => app.current_screen = Screen::Services,
                2 => app.current_screen = Screen::Adapters,
                3 => app.current_screen = Screen::Training,
                4 => app.current_screen = Screen::Chat,
                5 => app.current_screen = Screen::Logs,
                6 => app.current_screen = Screen::Metrics,
                7 => app.current_screen = Screen::Config,
                _ => app.current_screen = Screen::Help,
            }
        }

        // Randomly switch modes sometimes
        if rng.gen_bool(0.05) {
            match rng.gen_range(0..3) {
                0 => app.current_mode = Mode::Normal,
                1 => app.current_mode = Mode::ServiceSelect,
                _ => app.current_mode = Mode::ConfigEdit, // Skip chat input/filter for simplicity
            }
        }

        // Perform random action
        match rng.gen_range(0..4) {
            0 => app.on_up(),
            1 => app.on_down(),
            2 => app.on_left(),
            3 => app.on_right(),
            _ => {}
        }

        // 4. Assert Invariants

        // Invariant: selected_service must be within bounds if in ServiceSelect mode
        if app.current_mode == Mode::ServiceSelect {
            assert!(
                app.selected_service < app.services.len(),
                "on_down/up caused selected_service out of bounds at step {}",
                i
            );
        }

        // Invariant: selected_adapter must be within bounds if on Adapters screen
        if app.current_screen == Screen::Adapters && !app.adapters.is_empty() {
            assert!(
                app.selected_adapter < app.adapters.len(),
                "on_down/up caused selected_adapter out of bounds at step {}",
                i
            );
        }
    }
}
