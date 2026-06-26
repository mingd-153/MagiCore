pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/megagate.v1.rs"));
}

pub mod v1_ext {
    pub use crate::v1::*;
}

#[cfg(test)]
mod tests {
    use super::v1::*;

    #[test]
    fn test_package_ref_serialization() {
        let pkg_ref = PackageRef {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
        };
        let encoded = prost::Message::encode_to_vec(&pkg_ref);
        let decoded = PackageRef::decode(&*encoded).unwrap();
        assert_eq!(pkg_ref, decoded);
    }
}