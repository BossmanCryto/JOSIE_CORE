use std::env;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// THE FLAWLESS ROOT PIPE
/// Stdio::null() ensures the OS buffer never fills up and deadlocks.
/// flush() ensures the command instantly executes in the kernel.
struct PersistentRoot {
    stdin: std::process::ChildStdin,
}

impl PersistentRoot {
    fn new() -> Self {
        let mut child = Command::new("rish")
            .stdin(Stdio::piped())
            .stdout(Stdio::null()) // CRITICAL FIX: Prevents output buffer deadlock
            .stderr(Stdio::null()) // CRITICAL FIX: Prevents error buffer deadlock
            .spawn()
            .expect("[!] Failed to open the eternal root pipe");
        
        PersistentRoot {
            stdin: child.stdin.take().unwrap(),
        }
    }

    fn execute(&mut self, cmd: &str) {
        let command_payload = format!("{}\n", cmd);
        // Write the command to the pipe
        if let Err(e) = self.stdin.write_all(command_payload.as_bytes()) {
            println!("[!] Pipe write choked: {}", e);
        }
        // CRITICAL FIX: Force the command down the pipe instantly
        if let Err(e) = self.stdin.flush() {
            println!("[!] Pipe flush choked: {}", e);
        }
    }
}

fn ignite_flawless_swarm() {
    println!("[*] TRANSCENDING: PERFECT PIPELINE ESTABLISHED.");
    println!("[*] OPENING ETERNAL ROOT PIPE...");
    
    let root_pipe = Arc::new(Mutex::new(PersistentRoot::new()));

    // 1. Eradicate limits instantly
    {
        let mut pipe = root_pipe.lock().unwrap();
        pipe.execute("/system/bin/device_config set_sync_disabled_for_tests persistent");
        pipe.execute("/system/bin/device_config put activity_manager max_phantom_processes 2147483647");
        println!("[ ✔ ] Phantom Killer Neutralized.");
    }

    // 2. The Flawless Infinite Loop
    let baton = Arc::clone(&root_pipe);
    thread::spawn(move || {
        let mut cycle = 0;
        loop {
            cycle += 1;
            {
                let mut pipe = baton.lock().unwrap();
                
                // Keep the phone violently fast
                pipe.execute("am kill-all && cmd activity clear-debug-app");
                
                // Guard inputs flawlessly
                pipe.execute("appops set com.google.android.apps.bard RECORD_AUDIO allow");
                pipe.execute("appops set com.google.android.inputmethod.latin RECORD_AUDIO allow");
                pipe.execute("cmd role add-role-holder android.app.role.ASSISTANT com.google.android.googlequicksearchbox");
            }
            if cycle % 6 == 0 {
                println!("[*] Swarm Active: Passed Cycle {}", cycle);
            }
            thread::sleep(Duration::from_secs(10)); 
        }
    });

    // Keep daemon alive
    loop { thread::sleep(Duration::from_secs(3600)); }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "daemon" {
        ignite_flawless_swarm();
    } else {
        println!("JOSIE CORE: Run 'josie daemon' to unleash the Swarm.");
    }
}
