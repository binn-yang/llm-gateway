//! Structured Query DSL for observability data
//!
//! Provides a flexible query language for complex filtering and aggregation.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Query type enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryType {
    Logs,
    Spans,
    Metrics,
}

/// Filter operator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterOp {
    /// Equal (=)
    Eq,
    /// Not equal (!=)
    Ne,
    /// Greater than (>)
    Gt,
    /// Greater than or equal (>=)
    Gte,
    /// Less than (<)
    Lt,
    /// Less than or equal (<=)
    Lte,
    /// Contains (LIKE %value%)
    Contains,
    /// Starts with (LIKE value%)
    StartsWith,
    /// Ends with (LIKE %value)
    EndsWith,
    /// In list
    In,
}

/// Filter condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// Field name (e.g., "level", "timestamp", "provider")
    pub field: String,

    /// Operator
    pub op: FilterOp,

    /// Value (string, number, array)
    pub value: serde_json::Value,
}

/// Aggregation function
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggregationFunc {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

/// Aggregation specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregation {
    /// Aggregation function
    pub func: AggregationFunc,

    /// Field to aggregate (None for count)
    pub field: Option<String>,

    /// Alias for result
    pub alias: Option<String>,
}

/// Structured query DSL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryDSL {
    /// Query type (logs, spans, metrics)
    pub query_type: QueryType,

    /// Filter conditions (AND logic)
    #[serde(default)]
    pub filters: Vec<Filter>,

    /// Group by fields
    #[serde(default)]
    pub group_by: Vec<String>,

    /// Aggregations
    #[serde(default)]
    pub aggregations: Vec<Aggregation>,

    /// Sort by field
    pub sort_by: Option<String>,

    /// Sort order (asc/desc)
    #[serde(default = "default_sort_order")]
    pub sort_order: String,

    /// Limit results
    #[serde(default = "default_query_limit")]
    pub limit: usize,

    /// Offset for pagination
    #[serde(default)]
    pub offset: usize,
}

fn default_sort_order() -> String {
    "desc".to_string()
}

fn default_query_limit() -> usize {
    100
}

/// Query result
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResult {
    /// Query type
    pub query_type: QueryType,

    /// Result data (flexible JSON)
    pub results: serde_json::Value,

    /// Total count (before limit/offset)
    pub total: usize,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

impl QueryDSL {
    /// Validate the query
    pub fn validate(&self) -> Result<()> {
        // Check filters
        for filter in &self.filters {
            if filter.field.is_empty() {
                anyhow::bail!("Filter field cannot be empty");
            }
        }

        // Check group by with aggregations
        if !self.group_by.is_empty() && self.aggregations.is_empty() {
            anyhow::bail!("GROUP BY requires at least one aggregation");
        }

        // Check limit
        if self.limit == 0 {
            anyhow::bail!("Limit must be greater than 0");
        }

        if self.limit > 10000 {
            anyhow::bail!("Limit cannot exceed 10000");
        }

        Ok(())
    }

    /// Convert to SQL WHERE clause (simplified implementation)
    pub fn to_sql_where(&self) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        for filter in &self.filters {
            let condition = match &filter.op {
                FilterOp::Eq => format!("{} = ?", filter.field),
                FilterOp::Ne => format!("{} != ?", filter.field),
                FilterOp::Gt => format!("{} > ?", filter.field),
                FilterOp::Gte => format!("{} >= ?", filter.field),
                FilterOp::Lt => format!("{} < ?", filter.field),
                FilterOp::Lte => format!("{} <= ?", filter.field),
                FilterOp::Contains => format!("{} LIKE ?", filter.field),
                FilterOp::StartsWith => format!("{} LIKE ?", filter.field),
                FilterOp::EndsWith => format!("{} LIKE ?", filter.field),
                FilterOp::In => {
                    if let Some(arr) = filter.value.as_array() {
                        let placeholders = arr.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                        format!("{} IN ({})", filter.field, placeholders)
                    } else {
                        format!("{} = ?", filter.field)
                    }
                }
            };

            conditions.push(condition);

            // Add parameter value
            let param_value = match &filter.op {
                FilterOp::Contains => format!("%{}%", filter.value.as_str().unwrap_or("")),
                FilterOp::StartsWith => format!("{}%", filter.value.as_str().unwrap_or("")),
                FilterOp::EndsWith => format!("%{}", filter.value.as_str().unwrap_or("")),
                FilterOp::In => {
                    if let Some(arr) = filter.value.as_array() {
                        for val in arr {
                            params.push(val.to_string());
                        }
                        continue;
                    }
                    filter.value.to_string()
                }
                _ => filter.value.to_string(),
            };

            params.push(param_value);
        }

        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };

        (where_clause, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_dsl_defaults() {
        let json = r#"{"query_type": "logs"}"#;
        let query: QueryDSL = serde_json::from_str(json).unwrap();
        assert_eq!(query.limit, 100);
        assert_eq!(query.sort_order, "desc");
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn test_query_dsl_validation() {
        let query = QueryDSL {
            query_type: QueryType::Logs,
            filters: vec![],
            group_by: vec![],
            aggregations: vec![],
            sort_by: None,
            sort_order: "desc".to_string(),
            limit: 100,
            offset: 0,
        };

        assert!(query.validate().is_ok());
    }

    #[test]
    fn test_query_dsl_invalid_limit() {
        let query = QueryDSL {
            query_type: QueryType::Logs,
            filters: vec![],
            group_by: vec![],
            aggregations: vec![],
            sort_by: None,
            sort_order: "desc".to_string(),
            limit: 20000,
            offset: 0,
        };

        assert!(query.validate().is_err());
    }

    #[test]
    fn test_to_sql_where_simple() {
        let query = QueryDSL {
            query_type: QueryType::Logs,
            filters: vec![Filter {
                field: "level".to_string(),
                op: FilterOp::Eq,
                value: serde_json::json!("ERROR"),
            }],
            group_by: vec![],
            aggregations: vec![],
            sort_by: None,
            sort_order: "desc".to_string(),
            limit: 100,
            offset: 0,
        };

        let (where_clause, params) = query.to_sql_where();
        assert_eq!(where_clause, "level = ?");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_filter_op_contains() {
        let query = QueryDSL {
            query_type: QueryType::Logs,
            filters: vec![Filter {
                field: "message".to_string(),
                op: FilterOp::Contains,
                value: serde_json::json!("error"),
            }],
            group_by: vec![],
            aggregations: vec![],
            sort_by: None,
            sort_order: "desc".to_string(),
            limit: 100,
            offset: 0,
        };

        let (where_clause, params) = query.to_sql_where();
        assert_eq!(where_clause, "message LIKE ?");
        assert!(params[0].contains("%error%"));
    }
}
