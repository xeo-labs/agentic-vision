use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Query a capture by description and pass the result to Ollama.
    let vision = std::env::var("AVIS_PATH").unwrap_or_else(|_| "test.avis".to_string());
    let description = std::env::var("AVIS_DESC").unwrap_or_else(|_| "logo".to_string());

    let vision_out = Command::new("agentic-vision")
        .args(["query", "--description", &description, "--vision", &vision])
        .output()?;

    if !vision_out.status.success() {
        eprintln!(
            "vision query failed: {}",
            String::from_utf8_lossy(&vision_out.stderr)
        );
        std::process::exit(1);
    }

    let context = String::from_utf8_lossy(&vision_out.stdout);
    let prompt = format!(
        "Describe what this visual-memory result implies:\n{}",
        context.trim()
    );

    let status = Command::new("ollama")
        .args(["run", "llama3", &prompt])
        .status()?;

    if !status.success() {
        eprintln!("ollama run failed");
        std::process::exit(1);
    }

    Ok(())
}
