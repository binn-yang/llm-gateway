use serde::{Deserialize, Serialize};

/// Warning collected during request/response conversion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    /// Warning level ("warning" or "info")
    pub level: String,
    /// Warning message describing what was lost or changed
    pub message: String,
}

/// Collection of conversion warnings
#[derive(Debug, Clone, Default)]
pub struct ConversionWarnings {
    warnings: Vec<Warning>,
}

impl ConversionWarnings {
    /// Create a new empty warnings collection
    pub fn new() -> Self {
        Self {
            warnings: Vec::new(),
        }
    }

    /// Add a warning about an unsupported parameter
    pub fn add_unsupported_param(&mut self, param: &str, provider: &str) {
        self.warnings.push(Warning {
            level: "warning".to_string(),
            message: format!(
                "Parameter '{}' not supported by {} provider, ignoring",
                param, provider
            ),
        });
    }

    /// Add a warning about content loss during conversion
    pub fn add_content_loss(&mut self, content_type: &str, reason: &str) {
        self.warnings.push(Warning {
            level: "warning".to_string(),
            message: format!("Content type '{}' lost during conversion: {}", content_type, reason),
        });
    }

    /// Add an informational message
    pub fn add_info(&mut self, message: String) {
        self.warnings.push(Warning {
            level: "info".to_string(),
            message,
        });
    }

    /// Add a custom warning
    pub fn add_warning(&mut self, message: String) {
        self.warnings.push(Warning {
            level: "warning".to_string(),
            message,
        });
    }

    /// Check if there are any warnings
    pub fn is_empty(&self) -> bool {
        self.warnings.is_empty()
    }

    /// Get the number of warnings
    pub fn len(&self) -> usize {
        self.warnings.len()
    }

    /// Get all warnings
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Convert warnings to JSON string for HTTP header
    pub fn to_header_value(&self) -> Option<String> {
        if self.warnings.is_empty() {
            return None;
        }

        serde_json::to_string(&self.warnings).ok()
    }

    /// Merge another set of warnings into this one
    pub fn merge(&mut self, other: ConversionWarnings) {
        self.warnings.extend(other.warnings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_unsupported_param() {
        let mut warnings = ConversionWarnings::new();
        warnings.add_unsupported_param("seed", "Anthropic");

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings.warnings()[0].level, "warning");
        assert!(warnings.warnings()[0]
            .message
            .contains("seed"));
        assert!(warnings.warnings()[0]
            .message
            .contains("Anthropic"));
    }

    #[test]
    fn test_add_content_loss() {
        let mut warnings = ConversionWarnings::new();
        warnings.add_content_loss("tool_result", "not supported by provider");

        assert_eq!(warnings.len(), 1);
        assert!(warnings.warnings()[0]
            .message
            .contains("tool_result"));
    }

    #[test]
    fn test_to_header_value() {
        let mut warnings = ConversionWarnings::new();
        warnings.add_warning("Test warning".to_string());

        let header = warnings.to_header_value().unwrap();
        assert!(header.contains("Test warning"));
        assert!(header.contains("warning"));
    }

    #[test]
    fn test_empty_warnings() {
        let warnings = ConversionWarnings::new();
        assert!(warnings.is_empty());
        assert_eq!(warnings.len(), 0);
        assert!(warnings.to_header_value().is_none());
    }

    #[test]
    fn test_merge() {
        let mut warnings1 = ConversionWarnings::new();
        warnings1.add_warning("Warning 1".to_string());

        let mut warnings2 = ConversionWarnings::new();
        warnings2.add_warning("Warning 2".to_string());

        warnings1.merge(warnings2);
        assert_eq!(warnings1.len(), 2);
    }
}
