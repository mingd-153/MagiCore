/// Build colored MegaGate help text.
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
        "MegaGate - Universal Package Manager".white().bold()
    ));
    out.push_str(&format!(
        "  {}\n",
        "Usage: mg [OPTIONS] [COMMAND]".cyan().italic()
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

/// Print colored MegaGate help to stdout.
pub fn print_custom_help(available_cores: &[(&str, &str)]) {
    print!("{}", custom_help_text(available_cores));
}

#[cfg(test)]
mod tests {
    use super::custom_help_text;

    #[test]
    fn single_core_help_uses_bare_commands() {
        let help = custom_help_text(&[("web", "Web")]);
        assert!(help.contains("Build:"));
        assert!(help.contains("single-core (web)"));
        assert!(help.contains("create"));
        assert!(help.contains("dev"));
        assert!(help.contains("install"));
        assert!(help.contains("add"));
        assert!(!help.contains("create-web"));
        assert!(!help.contains("install-web"));
        assert!(!help.contains("add-web"));
    }

    #[test]
    fn multi_core_help_uses_per_core_web_commands() {
        let help = custom_help_text(&[("web", "Web"), ("ai", "AI")]);
        assert!(help.contains("multi-core (web, ai)"));
        assert!(help.contains("create-web"));
        assert!(help.contains("dev"));
        assert!(help.contains("install-web"));
        assert!(help.contains("install-ai"));
        assert!(help.contains("add-web"));
        assert!(help.contains("create-ai"));
        assert!(help.contains("add-ai"));
        assert!(!help.contains("── Single-core ──"));
    }

    #[test]
    fn single_core_non_web_help_keeps_per_core_surface() {
        let help = custom_help_text(&[("ai", "AI")]);
        assert!(help.contains("single-core (ai)"));
        assert!(help.contains("create-ai"));
        assert!(help.contains("dev"));
        assert!(help.contains("install-ai"));
        assert!(help.contains("add-ai"));
        assert!(!help.contains("    create   Scaffold a new web project"));
        assert!(!help.contains("── Single-core ──"));
    }
}
