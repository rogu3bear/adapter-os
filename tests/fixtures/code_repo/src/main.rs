mod safe_mode;
mod telemetry;

fn main() {
    println!("Safe Mode CLI placeholder");
    safe_mode::toggle_safe_mode();
    telemetry::report_safe_mode();
}
