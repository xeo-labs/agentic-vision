//! WQL query planner — converts AST into execution plan.

use crate::compiler::unifier::UnifiedSchema;
use crate::wql::parser::*;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// An execution plan for a WQL query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPlan {
    /// Ordered steps to execute.
    pub steps: Vec<PlanStep>,
}

/// A single step in the execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanStep {
    /// Scan all instances of a model across specified domains.
    ScanModel { model: String, domains: Vec<String> },
    /// Filter rows by a WHERE expression.
    Filter {
        field: String,
        op: String,
        value: String,
    },
    /// Join with another model.
    Join {
        target_model: String,
        relationship: String,
    },
    /// Enrich rows with temporal data.
    TemporalEnrich {
        fields: Vec<(String, String)>, // (field_name, temporal_func_name)
    },
    /// Sort rows.
    Sort { field: String, ascending: bool },
    /// Limit results.
    Limit { n: usize },
    /// Select specific fields.
    Project { fields: Vec<String> },
}

/// Plan a WQL query for execution.
pub fn plan(query: &WqlQuery, unified_schema: Option<&UnifiedSchema>) -> Result<QueryPlan> {
    let mut steps: Vec<PlanStep> = Vec::new();

    // Determine target domains
    let domains = if let Some(ref across) = query.across {
        across.clone()
    } else if let Some(schema) = unified_schema {
        schema.domains.clone()
    } else {
        Vec::new() // Will scan all available
    };

    // Step 1: Scan the model
    steps.push(PlanStep::ScanModel {
        model: query.from.name.clone(),
        domains,
    });

    // Step 2: Apply WHERE filters
    if let Some(ref where_clause) = query.where_clause {
        flatten_where(where_clause, &mut steps);
    }

    // Step 3: Apply JOINs
    for join in &query.joins {
        steps.push(PlanStep::Join {
            target_model: join.target_model.clone(),
            relationship: join.on_field.clone(),
        });
    }

    // Step 4: Temporal enrichment
    let temporal_fields: Vec<(String, String)> = query
        .select
        .iter()
        .filter(|f| f.temporal_func.is_some())
        .map(|f| {
            let func_name = match &f.temporal_func {
                Some(TemporalFunc::ValueAgo(days)) => format!("value_ago_{days}"),
                Some(TemporalFunc::Trend) => "trend".to_string(),
                Some(TemporalFunc::Predicted(days)) => format!("predicted_{days}"),
                Some(TemporalFunc::BestHistoric) => "best_historic".to_string(),
                Some(TemporalFunc::BestHistoricDomain) => "best_historic_domain".to_string(),
                None => String::new(),
            };
            (f.name.clone(), func_name)
        })
        .collect();

    if !temporal_fields.is_empty() {
        steps.push(PlanStep::TemporalEnrich {
            fields: temporal_fields,
        });
    }

    // Step 5: Sort
    if let Some(ref order_by) = query.order_by {
        for ob in order_by {
            steps.push(PlanStep::Sort {
                field: ob.field.clone(),
                ascending: ob.ascending,
            });
        }
    }

    // Step 6: Limit
    if let Some(limit) = query.limit {
        steps.push(PlanStep::Limit { n: limit });
    }

    // Step 7: Project fields
    if !query.select.iter().any(|f| f.name == "*") {
        let field_names: Vec<String> = query
            .select
            .iter()
            .map(|f| f.alias.clone().unwrap_or_else(|| f.name.clone()))
            .collect();
        steps.push(PlanStep::Project {
            fields: field_names,
        });
    }

    Ok(QueryPlan { steps })
}

/// Flatten a WHERE expression tree into filter steps.
fn flatten_where(expr: &WhereExpr, steps: &mut Vec<PlanStep>) {
    match expr {
        WhereExpr::Comparison { field, op, value } => {
            let op_str = match op {
                ComparisonOp::Eq => "=",
                ComparisonOp::Lt => "<",
                ComparisonOp::Gt => ">",
                ComparisonOp::Lte => "<=",
                ComparisonOp::Gte => ">=",
                ComparisonOp::Neq => "!=",
            };
            let val_str = match value {
                WqlValue::Float(f) => format!("{f}"),
                WqlValue::Integer(i) => format!("{i}"),
                WqlValue::String(s) => s.clone(),
                WqlValue::Bool(b) => format!("{b}"),
            };
            steps.push(PlanStep::Filter {
                field: field.clone(),
                op: op_str.to_string(),
                value: val_str,
            });
        }
        WhereExpr::And(left, right) => {
            flatten_where(left, steps);
            flatten_where(right, steps);
        }
        WhereExpr::Or(left, right) => {
            // OR is harder to flatten — emit as single filter for now
            flatten_where(left, steps);
            flatten_where(right, steps);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wql::parser;

    #[test]
    fn test_plan_simple_query() {
        let query = parser::parse("SELECT name, price FROM Product LIMIT 10").unwrap();
        let plan_result = plan(&query, None).unwrap();

        assert!(plan_result
            .steps
            .iter()
            .any(|s| matches!(s, PlanStep::ScanModel { model, .. } if model == "Product")));
        assert!(plan_result
            .steps
            .iter()
            .any(|s| matches!(s, PlanStep::Limit { n: 10 })));
    }

    #[test]
    fn test_plan_with_filter() {
        let query = parser::parse("SELECT name FROM Product WHERE price < 200 LIMIT 10").unwrap();
        let plan_result = plan(&query, None).unwrap();

        assert!(plan_result.steps.iter().any(
            |s| matches!(s, PlanStep::Filter { field, op, .. } if field == "price" && op == "<")
        ));
    }

    #[test]
    fn test_plan_with_order() {
        let query = parser::parse("SELECT name FROM Product ORDER BY price ASC LIMIT 10").unwrap();
        let plan_result = plan(&query, None).unwrap();

        assert!(plan_result
            .steps
            .iter()
            .any(|s| matches!(s, PlanStep::Sort { field, ascending: true } if field == "price")));
    }
}
