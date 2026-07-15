use serde::Serialize;

#[derive(Serialize)]
#[allow(dead_code)]
pub struct Status {
    pub service: String,
    pub version: String,
}

#[allow(dead_code)]
pub fn get_status() -> Status {
    Status {
        service: "{{project_name}}".to_string(),
        version: "0.1.0".to_string(),
    }
}
