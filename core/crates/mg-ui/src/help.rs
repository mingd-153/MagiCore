/// Print colored MegaGate help to stdout.
pub fn print_custom_help() {
    use colored::Colorize;
    println!();
    println!(
        "  {}  {}",
        "🚀".bold(),
        "MegaGate - Universal Package Manager".white().bold()
    );
    println!("  {}", "Usage: mg [OPTIONS] [COMMAND]".cyan().italic());
    println!();
    println!("  {}", "── Common ──".white().bold());
    println!(
        "    {}   {}",
        "init".cyan().bold(),
        "Interactive project wizard".white()
    );
    println!(
        "    {}   {}",
        "install".cyan().bold(),
        "Install all dependencies".white()
    );
    println!(
        "    {}   {}",
        "info".cyan().bold(),
        "Show package information".white()
    );
    println!(
        "    {}   {}",
        "search".cyan().bold(),
        "Search for packages".white()
    );
    println!(
        "    {}   {}",
        "outdated".cyan().bold(),
        "Check for outdated packages".white()
    );
    println!(
        "    {}   {}",
        "audit".cyan().bold(),
        "Audit packages for vulnerabilities".white()
    );
    println!();
    println!("  {}", "── Single-core (has .megagate/) ──".white().bold());
    println!(
        "    {}   {}",
        "add".green().bold(),
        "Add a dependency".white()
    );
    println!(
        "    {}   {}",
        "remove".green().bold(),
        "Remove a dependency".white()
    );
    println!(
        "    {}   {}",
        "update".green().bold(),
        "Update packages".white()
    );
    println!(
        "    {}   {}",
        "list".green().bold(),
        "List installed packages".white()
    );
    println!();
    println!("  {}", "── Quick create ──".white().bold());
    println!(
        "    {}   {}",
        "create-web".yellow().bold(),
        "Scaffold a new web project".white()
    );
    println!(
        "    {}   {}",
        "create-game".yellow().bold(),
        "Scaffold a new game project".white()
    );
    println!(
        "    {}   {}",
        "create-ai".yellow().bold(),
        "Scaffold a new AI project".white()
    );
    println!(
        "    {}   {}",
        "create-clo".yellow().bold(),
        "Scaffold a new cloud project".white()
    );
    println!(
        "    {}   {}",
        "create-cicd".yellow().bold(),
        "Scaffold a new CI/CD project".white()
    );
    println!(
        "    {}   {}",
        "create-iot".yellow().bold(),
        "Scaffold a new IoT project".white()
    );
    println!(
        "    {}   {}",
        "create-app".yellow().bold(),
        "Scaffold a new app project".white()
    );
    println!(
        "    {}   {}",
        "create-lib".yellow().bold(),
        "Scaffold a new library project".white()
    );
    println!();
    println!(
        "  {}",
        "── Per-core (global mode, no .megagate/) ──".white().bold()
    );
    println!("  {}", "  add:".green().bold());
    println!(
        "    {}  {}",
        "add-web".green().bold(),
        "Add web dependency".white()
    );
    println!(
        "    {}  {}",
        "add-game".green().bold(),
        "Add game dependency".white()
    );
    println!(
        "    {}  {}",
        "add-ai".green().bold(),
        "Add AI dependency".white()
    );
    println!(
        "    {}  {}",
        "add-clo".green().bold(),
        "Add cloud dependency".white()
    );
    println!(
        "    {}  {}",
        "add-cicd".green().bold(),
        "Add CI/CD dependency".white()
    );
    println!(
        "    {}  {}",
        "add-iot".green().bold(),
        "Add IoT dependency".white()
    );
    println!(
        "    {}  {}",
        "add-app".green().bold(),
        "Add app dependency".white()
    );
    println!(
        "    {}  {}",
        "add-lib".green().bold(),
        "Add library dependency".white()
    );
    println!("  {}", "  remove:".red().bold());
    println!(
        "    {}  {}",
        "remove-web".red().bold(),
        "Remove web dependency".white()
    );
    println!(
        "    {}  {}",
        "remove-game".red().bold(),
        "Remove game dependency".white()
    );
    println!(
        "    {}  {}",
        "remove-ai".red().bold(),
        "Remove AI dependency".white()
    );
    println!(
        "    {}  {}",
        "remove-clo".red().bold(),
        "Remove cloud dependency".white()
    );
    println!(
        "    {}  {}",
        "remove-cicd".red().bold(),
        "Remove CI/CD dependency".white()
    );
    println!(
        "    {}  {}",
        "remove-iot".red().bold(),
        "Remove IoT dependency".white()
    );
    println!(
        "    {}  {}",
        "remove-app".red().bold(),
        "Remove app dependency".white()
    );
    println!(
        "    {}  {}",
        "remove-lib".red().bold(),
        "Remove library dependency".white()
    );
    println!("  {}", "  list:".blue().bold());
    println!(
        "    {}  {}",
        "list-web".blue().bold(),
        "List web packages".white()
    );
    println!(
        "    {}  {}",
        "list-game".blue().bold(),
        "List game packages".white()
    );
    println!(
        "    {}  {}",
        "list-ai".blue().bold(),
        "List AI packages".white()
    );
    println!(
        "    {}  {}",
        "list-clo".blue().bold(),
        "List cloud packages".white()
    );
    println!(
        "    {}  {}",
        "list-cicd".blue().bold(),
        "List CI/CD packages".white()
    );
    println!(
        "    {}  {}",
        "list-iot".blue().bold(),
        "List IoT packages".white()
    );
    println!(
        "    {}  {}",
        "list-app".blue().bold(),
        "List app packages".white()
    );
    println!(
        "    {}  {}",
        "list-lib".blue().bold(),
        "List library packages".white()
    );
    println!("  {}", "  update:".magenta().bold());
    println!(
        "    {}  {}",
        "update-web".magenta().bold(),
        "Update web packages".white()
    );
    println!(
        "    {}  {}",
        "update-game".magenta().bold(),
        "Update game packages".white()
    );
    println!(
        "    {}  {}",
        "update-ai".magenta().bold(),
        "Update AI packages".white()
    );
    println!(
        "    {}  {}",
        "update-clo".magenta().bold(),
        "Update cloud packages".white()
    );
    println!(
        "    {}  {}",
        "update-cicd".magenta().bold(),
        "Update CI/CD packages".white()
    );
    println!(
        "    {}  {}",
        "update-iot".magenta().bold(),
        "Update IoT packages".white()
    );
    println!(
        "    {}  {}",
        "update-app".magenta().bold(),
        "Update app packages".white()
    );
    println!(
        "    {}  {}",
        "update-lib".magenta().bold(),
        "Update library packages".white()
    );
    println!();
    println!("  {}", "── Options ──".white().bold());
    println!(
        "    {}   {}",
        "--core <CORE>".cyan().bold(),
        "Target core (web, game, ai, clo, cicd, iot, app, lib)".white()
    );
    println!(
        "    {}   {}",
        "-h, --help".cyan().bold(),
        "Print help (per-command)".white()
    );
    println!(
        "    {}   {}",
        "-V, --version".cyan().bold(),
        "Print version".white()
    );
    println!();
}
