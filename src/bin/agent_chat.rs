use agent_memory::conversation::append_message;
use anyhow::Result;
use std::io::{self, Write};

fn main() -> Result<()> {
    println!("=== Trellis Chat Session (auto‑logged) ===");
    println!("Type your message and press Enter. Type 'exit' to quit.");

    loop {
        // Prompt user
        print!("You: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let msg = input.trim();
        if msg.eq_ignore_ascii_case("exit") || msg.is_empty() {
            break;
        }
        // Log user message
        append_message("user", msg)?;

        // Simple placeholder agent response (you can replace with real AI call)
        let response = format!("Echo: {}", msg);
        println!("Agent: {}", response);
        // Log agent response
        append_message("agent", &response)?;
    }

    // Khi kết thúc, ghi lại một tin nhắn kết thúc phiên
    append_message("agent", "Session ended")?;
    println!("Session saved to .trellis/workspace/conversation.json");
    Ok(())
}
