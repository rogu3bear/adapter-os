use crate::components::icons::*;
use leptos::prelude::*;
use std::time::Duration;

#[component]
pub fn BootSequence(#[prop(into)] on_complete: Callback<()>) -> impl IntoView {
    let (stage, set_stage) = signal(0);
    let (logs, set_logs) = signal(Vec::<String>::new());

    // Effect to sequence the boot steps
    Effect::new(move |_| {
        let current = stage.get();
        match current {
            0 => {
                set_logs.update(|l| {
                    l.push("BOOT: Initializing adapterOS Secure Kernel...".to_string())
                });
                leptos::prelude::set_timeout(move || set_stage.set(1), Duration::from_millis(600));
            }
            1 => {
                set_logs
                    .update(|l| l.push("AUTH: Cryptographic Identity Verification...".to_string()));
                leptos::prelude::set_timeout(
                    move || {
                        set_logs.update(|l| {
                            l.push("AUTH: [OK] Operator Identity Verified: ADMIN-0".to_string())
                        });
                        set_stage.set(2);
                    },
                    Duration::from_millis(1000),
                );
            }
            2 => {
                set_logs.update(|l| {
                    l.push("HAL: Loading Metal Performance Shaders (MPS)...".to_string())
                });
                leptos::prelude::set_timeout(move || set_stage.set(3), Duration::from_millis(800));
            }
            3 => {
                set_logs.update(|l| l.push("MEM: Allocating UMA segments...".to_string()));
                leptos::prelude::set_timeout(
                    move || {
                        set_logs.update(|l| l.push("SYS: System Ready.".to_string()));
                        set_stage.set(4);
                    },
                    Duration::from_millis(700),
                );
            }
            4 => {
                leptos::prelude::set_timeout(
                    move || on_complete.run(()),
                    Duration::from_millis(1000),
                );
            }
            _ => {}
        }
    });

    view! {
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-background overflow-hidden">
            <style>
                "
                @keyframes scan {
                    0% { transform: translateY(-100%); opacity: 0; }
                    50% { opacity: 0.5; }
                    100% { transform: translateY(100vh); opacity: 0; }
                }
                .animate-scan {
                    animation: scan 3s linear infinite;
                }
                "
            </style>

            // Biometric scan light effect
            <div class="absolute inset-0 pointer-events-none overflow-hidden">
                <div class="absolute w-full h-1 bg-primary/20 shadow-[0_0_15px_rgba(var(--primary),0.5)] animate-scan"></div>
            </div>

            // Main Boot Content
            <div class="relative z-10 w-full max-w-md p-8 bg-card/10 backdrop-blur-xl border border-primary/20 rounded-xl flex flex-col items-center shadow-2xl">
                <div class="mb-8 relative">
                    <IconServer class="w-16 h-16 text-primary animate-pulse" />
                    <div class="absolute -inset-4 border border-primary/20 rounded-full animate-ping opacity-20"></div>
                </div>

                <div class="w-full space-y-4 font-mono text-sm leading-relaxed">
                    // Log output
                    <div class="h-48 overflow-y-auto space-y-1 text-primary/80 scrollbar-hide">
                        {move || logs.get().into_iter().map(|log| view! {
                            <div class="flex gap-2">
                                <span class="text-primary/40">">"</span>
                                <span>{log}</span>
                            </div>
                        }).collect_view()}
                        <div class="w-2 h-4 bg-primary animate-pulse inline-block align-middle ml-1"></div>
                    </div>

                    // Progress Bar
                    <div class="w-full h-1 bg-primary/10 rounded-full overflow-hidden">
                        <div
                            class="h-full bg-primary shadow-[0_0_8px_rgba(var(--primary),0.6)] transition-all duration-700 ease-out"
                            style=move || format!("width: {}%", (stage.get() as f32 / 4.0) * 100.0)
                        ></div>
                    </div>

                    <div class="flex justify-between items-center text-[10px] text-primary/30 uppercase tracking-widest mt-4">
                        <span>"adapterOS Kernel v4.12.0"</span>
                        <span>"SECURE_BOOT_ACTIVE"</span>
                    </div>
                </div>
            </div>
        </div>
    }
}
