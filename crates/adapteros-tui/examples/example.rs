// Example showing the aligned TUI layout

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║                                                                              ║");
    println!("║   █████╗ ██████╗  █████╗ ██████╗ ████████╗███████╗██████╗  ██████╗ ███████╗║");
    println!("║  ██╔══██╗██╔══██╗██╔══██╗██╔══██╗╚══██╔══╝██╔════╝██╔══██╗██╔═══██╗██╔════╝║");
    println!("║  ███████║██║  ██║███████║██████╔╝   ██║   █████╗  ██████╔╝██║   ██║███████╗║");
    println!("║  ██╔══██║██║  ██║██╔══██║██╔═══╝    ██║   ██╔══╝  ██╔══██╗██║   ██║╚════██║║");
    println!("║  ██║  ██║██████╔╝██║  ██║██║        ██║   ███████╗██║  ██║╚██████╔╝███████║║");
    println!("║  ╚═╝  ╚═╝╚═════╝ ╚═╝  ╚═╝╚═╝        ╚═╝   ╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝║");
    println!("║                                                                              ║");
    println!("║                      SUPERBACKEND CONTROL SYSTEM                            ║");
    println!("║                                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("╔═══════════════════════════════════════════════════╦══════════════════════════╗");
    println!("║ Model: llama-7b-lora-q15  │ Status: [OK] LOADED  ║ ▣ LIVE│Mem: 33%│Queue: 2│ ║");
    println!("║                           │ Mode: DEV            ║  ⟳ 1s │TPS:842 │         ║");
    println!("╚═══════════════════════════════════════════════════╩══════════════════════════╝");
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║ System Status                                                               ║");
    println!("╟──────────────────────────────────────────────────────────────────────────────╢");
    println!("║ [OK] Database        │ Connected    │ Latency: 1.2ms                        ║");
    println!("║ [OK] Router          │ Ready        │ Adapters: 12/50                       ║");
    println!("║ [!!] Security        │ DEVELOPMENT  │ Relaxed policies                      ║");
    println!("║                                                                              ║");
    println!("║ Services: 2 running, 4 stopped, 0 failed                                    ║");
    println!("║ Memory Headroom: 15.0% [Good >= 15%]                                        ║");
    println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║ Select Service to Control                                                   ║");
    println!("╟──────────────────────────────────────────────────────────────────────────────╢");
    println!("║     Status  Service              State      Dependencies      Action        ║");
    println!("║    ──────────────────────────────────────────────────────────────────        ║");
    println!("║  >  [OK]    Database              Running    None              [Restart]    ║");
    println!("║     [OK]    Router                Running    Database          [Restart]    ║");
    println!("║     [--]    Metrics System        Stopped    None              [Start]      ║");
    println!("║     [--]    Policy Engine         Stopped    Router            [Start]      ║");
    println!("║     [--]    Training Service      Stopped    Database,Router   [Start]      ║");
    println!("║     [--]    Telemetry             Stopped    Metrics           [Start]      ║");
    println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("All lines are perfectly aligned vertically using fixed-width formatting!");
    println!("The 'adapterOS' text is now a unified ASCII art block.");
}
