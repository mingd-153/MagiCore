/// Interactive prompts with styled UI
use anyhow::Result;
use console::style;
use dialoguer::{Confirm, Input, MultiSelect, Select};

fn prompt_style(text: &str) -> String {
    format!(
        "{} {}",
        style("◆").cyan().bold(),
        style(text).white().bold()
    )
}

/// Ask yes/no question
pub fn confirm(prompt: &str) -> Result<bool> {
    Ok(Confirm::new()
        .with_prompt(prompt_style(prompt))
        .default(true)
        .show_default(true)
        .interact()?)
}

/// Ask for text input with placeholder
pub fn input(prompt: &str) -> Result<String> {
    Ok(Input::new()
        .with_prompt(prompt_style(prompt))
        .interact_text()?)
}

/// Select one from list with styled items
pub fn select(prompt: &str, items: &[&str]) -> Result<usize> {
    let styled: Vec<String> = items.iter().map(|i| style(i).white().to_string()).collect();
    let refs: Vec<&str> = styled.iter().map(|s| s.as_str()).collect();
    Ok(Select::new()
        .with_prompt(prompt_style(prompt))
        .items(&refs)
        .default(0)
        .interact()?)
}

/// Select multiple from list (checkboxes)
pub fn multi_select(prompt: &str, items: &[&str]) -> Result<Vec<usize>> {
    let selected: Vec<String> = items
        .iter()
        .map(|item| format!("{} {}", style("☐").dim(), style(item).white()))
        .collect();
    let display: Vec<&str> = selected.iter().map(|s| s.as_str()).collect();
    Ok(MultiSelect::new()
        .with_prompt(prompt_style(prompt))
        .items(&display)
        .interact()?)
}
