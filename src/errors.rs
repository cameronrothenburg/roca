#[derive(Debug, Clone)]
pub struct RuleError {
    pub code: String,
    pub message: String,
    pub context: Option<String>,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "error[{}]: {}", self.code, self.message)?;
        if let Some(ctx) = &self.context {
            write!(f, "\n  → {}", ctx)?;
        }
        Ok(())
    }
}
