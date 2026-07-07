use console::style;

pub fn style_header(text: &str) -> String {
    style(text).cyan().bold().to_string()
}

pub fn style_success(text: &str) -> String {
    style(text).green().bold().to_string()
}

pub fn style_dim(text: &str) -> String {
    style(text).dim().to_string()
}

pub fn style_error(text: &str) -> String {
    style(text).red().bold().to_string()
}

pub fn style_info(text: &str) -> String {
    style(text).blue().to_string()
}
