use serde::{Deserialize, Serialize};

/// Model pricing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPrice {
    pub model_name: String,
    pub provider: String,
    pub input_price: f64,
    pub output_price: f64,
    pub cache_write_price: Option<f64>,
    pub cache_read_price: Option<f64>,
    pub currency: String,
    pub effective_date: String,
    pub notes: Option<String>,
}

/// Cost breakdown for a request
#[derive(Debug, Clone, Default)]
pub struct CostBreakdown {
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
    pub total_cost: f64,
}

impl CostBreakdown {
    /// Create a zero-cost breakdown
    pub fn zero() -> Self {
        Self::default()
    }

    /// Calculate total cost from components
    pub fn calculate_total(&mut self) {
        self.total_cost = self.input_cost
            + self.output_cost
            + self.cache_write_cost
            + self.cache_read_cost;
    }
}
