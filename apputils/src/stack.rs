/// A type wrapper to ensure name conforms to minimal expectations.
#[derive(Debug, Clone)]
pub struct Name(String);
impl Name {
    pub fn new(name: &str) -> Result<Self, &'static str> {
        let name = name.to_lowercase();

        if name.starts_with("-") || name.ends_with("-") {
            return Err("Name cannot start or end with dash");
        }

        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_new() {
        assert_eq!(Name::new("dc-ex1").unwrap().as_str(), "dc-ex1");
        assert_eq!(Name::new("DC-EX1").unwrap().as_str(), "dc-ex1");
        assert!(Name::new("-dc-ex1").is_err());
        assert!(Name::new("dc-ex1-").is_err());
    }
}
