use crate::error::NameValidationError;

pub struct NameValidator;

impl NameValidator {
    pub fn validate(name: &str) -> Result<(), NameValidationError> {
        if name.is_empty() {
            return Err(NameValidationError::Empty);
        }

        if name.len() > 214 {
            return Err(NameValidationError::TooLong);
        }

        let first = name.chars().next().ok_or(NameValidationError::Empty)?;
        if first == '.' || first == '_' {
            return Err(NameValidationError::InvalidStart);
        }

        if !name.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.' | '_' | '~')
        }) {
            return Err(NameValidationError::InvalidCharacters);
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_valid_names() {
        assert!(NameValidator::validate("my-app").is_ok());
        assert!(NameValidator::validate("test123").is_ok());
        assert!(NameValidator::validate("my.app").is_ok());
        assert!(NameValidator::validate("my_app").is_ok());
        assert!(NameValidator::validate("a~b").is_ok());
    }

    #[test]
    fn test_invalid_start() {
        assert!(matches!(
            NameValidator::validate(".hidden"),
            Err(NameValidationError::InvalidStart)
        ));
        assert!(matches!(
            NameValidator::validate("_private"),
            Err(NameValidationError::InvalidStart)
        ));
    }

    #[test]
    fn test_empty() {
        assert!(matches!(
            NameValidator::validate(""),
            Err(NameValidationError::Empty)
        ));
    }

    #[test]
    fn test_too_long() {
        let long = "a".repeat(215);
        assert!(matches!(
            NameValidator::validate(&long),
            Err(NameValidationError::TooLong)
        ));
    }

    #[test]
    fn test_uppercase_rejected() {
        assert!(matches!(
            NameValidator::validate("UPPERCASE"),
            Err(NameValidationError::InvalidCharacters)
        ));
    }

    #[test]
    fn test_space_rejected() {
        assert!(matches!(
            NameValidator::validate("my app"),
            Err(NameValidationError::InvalidCharacters)
        ));
    }

    #[test]
    fn test_special_chars_rejected() {
        assert!(matches!(
            NameValidator::validate("my@pp"),
            Err(NameValidationError::InvalidCharacters)
        ));
    }

    #[test]
    fn test_boundary_214() {
        let ok = "a".repeat(214);
        assert!(NameValidator::validate(&ok).is_ok());
    }
}
