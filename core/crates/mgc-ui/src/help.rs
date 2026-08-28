/// Build colored MagiCore help text.
/// `available_cores` — list of (short_name, label) from factory.
pub fn custom_help_text(available_cores: &[(&str, &str)]) -> String {
    use colored::Colorize;
    let has_web = available_cores.iter().any(|(s, _)| *s == "web");
    let single_core = available_cores.len() == 1;
    let single_core_web = single_core && has_web;
    let build_profile = if single_core {
        let core = available_cores
            .first()
            .map(|(core, _)| *core)
            .unwrap_or("unknown");
        format!("single-core ({core})")
    } else {
        let cores = available_cores
            .iter()
            .map(|(core, _)| *core)
            .collect::<Vec<_>>()
            .join(", ");
        format!("multi-core ({cores})")
    };
    let per_core_name = |prefix: &str, core: &str| format!("{prefix}-{core}");
    let mut out = String::new();
    out.push('\n');
    out.push_str(&format!(
        "  {}  {}\n",
        "🚀".bold(),
        "MagiCore - Universal Package Manager".white().bold()
    ));
    out.push_str(&format!(
        "  {}\n",
        "Usage: mgc [OPTIONS] [COMMAND]".cyan().italic()
    ));
    out.push_str(&format!(
        "  {} {}\n",
        "Build:".white().bold(),
        build_profile.white()
    ));
    out.push('\n');
    out.push_str(&format!("  {}\n", "── Common ──".white().bold()));
    out.push_str(&format!(
        "    {}   {}\n",
        "init".cyan().bold(),
        "Interactive project wizard".white()
    ));
    out.push_str(&format!(
        "    {}   {}\n",
        "dev".cyan().bold(),
        "Start the local development server".white()
    ));
    if single_core {
        out.push_str(&format!(
            "    {}   {}\n",
            "install".cyan().bold(),
            "Install all dependencies".white()
        ));
    } else {
        for (core, label) in available_cores {
            out.push_str(&format!(
                "    {}   {}\n",
                per_core_name("install", core).cyan().bold(),
                format!("Install dependencies for {}", label).white()
            ));
        }
    }
    out.push_str(&format!(
        "    {}   {}\n",
        "info".cyan().bold(),
        "Show package information".white()
    ));
    out.push_str(&format!(
        "    {}   {}\n",
        "search".cyan().bold(),
        "Search for packages".white()
    ));
    out.push_str(&format!(
        "    {}   {}\n",
        "outdated".cyan().bold(),
        "Check for outdated packages".white()
    ));
    out.push_str(&format!(
        "    {}   {}\n",
        "audit".cyan().bold(),
        "Audit packages for vulnerabilities".white()
    ));
    out.push('\n');

    if single_core_web {
        out.push_str(&format!("  {}\n", "── Single-core ──".white().bold()));
        out.push_str(&format!(
            "    {}   {}\n",
            "add".green().bold(),
            "Add a dependency".white()
        ));
        out.push_str(&format!(
            "    {}   {}\n",
            "remove".green().bold(),
            "Remove a dependency".white()
        ));
        out.push_str(&format!(
            "    {}   {}\n",
            "update".green().bold(),
            "Update packages".white()
        ));
        out.push_str(&format!(
            "    {}   {}\n",
            "list".green().bold(),
            "List installed packages".white()
        ));
        out.push('\n');
    }

    out.push_str(&format!("  {}\n", "── Create ──".white().bold()));
    if single_core_web {
        out.push_str(&format!(
            "    {}   {}\n",
            "create".yellow().bold(),
            "Scaffold a new web project".white()
        ));
    } else {
        for (core, label) in available_cores {
            out.push_str(&format!(
                "    {}   {}\n",
                per_core_name("create", core).yellow().bold(),
                format!("Scaffold {}", label).white()
            ));
        }
    }
    out.push('\n');

    if !single_core_web {
        out.push_str(&format!("  {}\n", "── Per-core Commands ──".white().bold()));
        for (core, label) in available_cores {
            out.push_str(&format!(
                "    {}  {}\n",
                per_core_name("install", core).cyan().bold(),
                format!("Install dependencies for {}", label).white()
            ));
            out.push_str(&format!(
                "    {}  {}\n",
                per_core_name("add", core).green().bold(),
                format!("Add dependency for {}", label).white()
            ));
            out.push_str(&format!(
                "    {}  {}\n",
                per_core_name("remove", core).red().bold(),
                format!("Remove dependency for {}", label).white()
            ));
            out.push_str(&format!(
                "    {}  {}\n",
                per_core_name("list", core).blue().bold(),
                format!("List packages for {}", label).white()
            ));
            out.push_str(&format!(
                "    {}  {}\n",
                per_core_name("update", core).magenta().bold(),
                format!("Update packages for {}", label).white()
            ));
        }
        out.push('\n');
    }

    out.push_str(&format!("  {}\n", "── Options ──".white().bold()));
    let cores_list = available_cores
        .iter()
        .map(|(s, _)| *s)
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!(
        "    {}   {}\n",
        "--core <CORE>".cyan().bold(),
        format!("Target core ({})", cores_list).white()
    ));
    out.push_str(&format!(
        "    {}   {}\n",
        "-h, --help".cyan().bold(),
        "Print help (per-command)".white()
    ));
    out.push_str(&format!(
        "    {}   {}\n",
        "-V, --version".cyan().bold(),
        "Print version".white()
    ));
    out.push('\n');
    out
}

/// Print colored MagiCore help to stdout.
pub fn print_custom_help(available_cores: &[(&str, &str)]) {
    print!("{}", custom_help_text(available_cores));
}

