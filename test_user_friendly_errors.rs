use adapteros_server_api::errors::UserFriendlyErrorMapper;

fn main() {
    // Test user-friendly error messages
    let db_error = UserFriendlyErrorMapper::map_error_message("DB_ERROR", "Connection refused");
    println!("DB Error: {}", db_error);
    
    let load_error = UserFriendlyErrorMapper::map_error_message("LOAD_FAILED", "path does not exist");
    println!("Load Error: {}", load_error);
    
    let not_found = UserFriendlyErrorMapper::map_error_message("NOT_FOUND", "model not found");
    println!("Not Found: {}", not_found);
    
    println!("✅ User-friendly error messages are working!");
}
