use serde::Serialize;

#[derive(Serialize)]
pub struct Status {
    pub service: String,
    pub version: String,
}

pub fn get_status() -> Status {
    Status {
        service: "{{project_name}}".to_string(),
        version: "0.1.0".to_string(),
    }
}
