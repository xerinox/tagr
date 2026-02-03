use sled;
use std::thread;
use std::time::Duration;

fn main() {
    let path = "tmp/test_db_lock";
    let _ = std::fs::remove_dir_all(path);

    // Process 1: Open DB
    match unsafe { mysys_fork::fork() } {
        Ok(0) => {
            // Child
            println!("Child: Opening DB...");
            let db = sled::open(path).unwrap();
            println!("Child: DB Opened. Sleeping...");
            thread::sleep(Duration::from_secs(2));
            println!("Child: Exiting.");
        }
        Ok(pid) => {
            // Parent
            thread::sleep(Duration::from_millis(500)); // Ensure child opens it
            println!("Parent: Attempting to open locked DB...");
            let start = std::time::Instant::now();
            let result = sled::open(path);
            let duration = start.elapsed();
            
            match result {
                Ok(_) => println!("Parent: Opened DB (Unexpected!)"),
                Err(e) => println!("Parent: Failed to open DB as expected: {:?} (Took {:?})", e, duration),
            }
            
            // Wait for child
             let mut status = 0;
            unsafe { libc::waitpid(pid, &mut status, 0) };
        }
        Err(_) => println!("Fork failed"),
    }
}

mod mysys_fork {
    extern "C" {
        pub fn fork() -> i32;
    }
}
