//! Watch system — alert rules that monitor temporal data.

use crate::temporal::query::TemporalQuery;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A watch rule that monitors a temporal query for conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchRule {
    /// Unique rule ID.
    pub id: String,
    /// Domain to watch.
    pub domain: String,
    /// Model type to watch (e.g., "Product").
    pub model_type: Option<String>,
    /// Feature dimension to monitor.
    pub feature_dim: u8,
    /// Condition that triggers the alert.
    pub condition: WatchCondition,
    /// Where to send notifications.
    pub notify: NotifyTarget,
    /// Whether this rule is active.
    pub active: bool,
    /// When this rule was created.
    pub created_at: DateTime<Utc>,
    /// When this rule last triggered.
    pub last_triggered: Option<DateTime<Utc>>,
}

/// Condition that triggers a watch alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatchCondition {
    /// Feature value goes above threshold.
    ValueAbove(f32),
    /// Feature value goes below threshold.
    ValueBelow(f32),
    /// Feature changes by more than a percentage threshold.
    ChangeByPercent(f32),
    /// A previously unavailable item becomes available.
    Available,
    /// A new node of the watched type appears.
    NewInstance,
}

/// Notification target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotifyTarget {
    /// Send to a webhook URL.
    Webhook(String),
    /// Emit on the event bus.
    EventBus,
    /// Send to connected protocol agents.
    Protocol,
}

/// Watch alert notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchAlert {
    /// Rule that triggered.
    pub rule_id: String,
    /// Domain.
    pub domain: String,
    /// What was detected.
    pub message: String,
    /// Current value.
    pub current_value: f32,
    /// Previous value (if applicable).
    pub previous_value: Option<f32>,
    /// When the alert was generated.
    pub timestamp: DateTime<Utc>,
}

/// Manages active watch rules.
pub struct WatchManager {
    /// Active rules.
    rules: HashMap<String, WatchRule>,
    /// Alerts generated.
    alerts: Vec<WatchAlert>,
}

impl WatchManager {
    /// Create a new watch manager.
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            alerts: Vec::new(),
        }
    }

    /// Add a new watch rule.
    pub fn add_rule(&mut self, rule: WatchRule) -> String {
        let id = rule.id.clone();
        self.rules.insert(id.clone(), rule);
        id
    }

    /// Remove a watch rule.
    pub fn remove_rule(&mut self, id: &str) -> bool {
        self.rules.remove(id).is_some()
    }

    /// List all active rules.
    pub fn list_rules(&self) -> Vec<&WatchRule> {
        self.rules.values().collect()
    }

    /// Evaluate all rules against new data.
    ///
    /// Returns any alerts that were triggered.
    pub fn evaluate(
        &mut self,
        domain: &str,
        feature_dim: u8,
        current_value: f32,
        previous_value: f32,
    ) -> Vec<WatchAlert> {
        let mut triggered = Vec::new();

        for rule in self.rules.values_mut() {
            if rule.domain != domain || rule.feature_dim != feature_dim || !rule.active {
                continue;
            }

            let alert = match &rule.condition {
                WatchCondition::ValueAbove(threshold) => {
                    if current_value > *threshold && previous_value <= *threshold {
                        Some(WatchAlert {
                            rule_id: rule.id.clone(),
                            domain: domain.to_string(),
                            message: format!("Value rose above {threshold}: {current_value}"),
                            current_value,
                            previous_value: Some(previous_value),
                            timestamp: Utc::now(),
                        })
                    } else {
                        None
                    }
                }
                WatchCondition::ValueBelow(threshold) => {
                    if current_value < *threshold && previous_value >= *threshold {
                        Some(WatchAlert {
                            rule_id: rule.id.clone(),
                            domain: domain.to_string(),
                            message: format!("Value dropped below {threshold}: {current_value}"),
                            current_value,
                            previous_value: Some(previous_value),
                            timestamp: Utc::now(),
                        })
                    } else {
                        None
                    }
                }
                WatchCondition::ChangeByPercent(pct) => {
                    if previous_value != 0.0 {
                        let change = ((current_value - previous_value) / previous_value).abs();
                        if change > *pct {
                            Some(WatchAlert {
                                rule_id: rule.id.clone(),
                                domain: domain.to_string(),
                                message: format!(
                                    "Value changed by {:.1}% (threshold: {:.1}%)",
                                    change * 100.0,
                                    pct * 100.0
                                ),
                                current_value,
                                previous_value: Some(previous_value),
                                timestamp: Utc::now(),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                WatchCondition::Available => {
                    if previous_value <= 0.0 && current_value > 0.0 {
                        Some(WatchAlert {
                            rule_id: rule.id.clone(),
                            domain: domain.to_string(),
                            message: "Item became available".to_string(),
                            current_value,
                            previous_value: Some(previous_value),
                            timestamp: Utc::now(),
                        })
                    } else {
                        None
                    }
                }
                WatchCondition::NewInstance => None, // Handled separately
            };

            if let Some(alert) = alert {
                rule.last_triggered = Some(Utc::now());
                triggered.push(alert);
            }
        }

        self.alerts.extend(triggered.clone());
        triggered
    }

    /// Get recent alerts.
    pub fn recent_alerts(&self, limit: usize) -> &[WatchAlert] {
        let start = if self.alerts.len() > limit {
            self.alerts.len() - limit
        } else {
            0
        };
        &self.alerts[start..]
    }
}

impl Default for WatchManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(condition: WatchCondition) -> WatchRule {
        WatchRule {
            id: "test-1".to_string(),
            domain: "shop.com".to_string(),
            model_type: Some("Product".to_string()),
            feature_dim: 48, // price
            condition,
            notify: NotifyTarget::EventBus,
            active: true,
            created_at: Utc::now(),
            last_triggered: None,
        }
    }

    #[test]
    fn test_watch_value_below() {
        let mut wm = WatchManager::new();
        wm.add_rule(make_rule(WatchCondition::ValueBelow(80.0)));

        // Price drops from 100 to 75
        let alerts = wm.evaluate("shop.com", 48, 75.0, 100.0);
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].message.contains("below"));
    }

    #[test]
    fn test_watch_value_above() {
        let mut wm = WatchManager::new();
        wm.add_rule(make_rule(WatchCondition::ValueAbove(150.0)));

        // Price rises from 100 to 200
        let alerts = wm.evaluate("shop.com", 48, 200.0, 100.0);
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn test_watch_no_trigger() {
        let mut wm = WatchManager::new();
        wm.add_rule(make_rule(WatchCondition::ValueBelow(80.0)));

        // Price stays above threshold
        let alerts = wm.evaluate("shop.com", 48, 100.0, 95.0);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_watch_change_by_percent() {
        let mut wm = WatchManager::new();
        wm.add_rule(make_rule(WatchCondition::ChangeByPercent(0.1))); // 10%

        // 20% change
        let alerts = wm.evaluate("shop.com", 48, 80.0, 100.0);
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn test_watch_available() {
        let mut wm = WatchManager::new();
        let mut rule = make_rule(WatchCondition::Available);
        rule.feature_dim = 51; // availability
        wm.add_rule(rule);

        let alerts = wm.evaluate("shop.com", 51, 1.0, 0.0);
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].message.contains("available"));
    }

    #[test]
    fn test_watch_manage_rules() {
        let mut wm = WatchManager::new();
        wm.add_rule(make_rule(WatchCondition::ValueBelow(80.0)));
        assert_eq!(wm.list_rules().len(), 1);

        wm.remove_rule("test-1");
        assert!(wm.list_rules().is_empty());
    }

    // ── v4 Test Suite: Phase 3D — Watch/Alert System ──

    #[test]
    fn test_v4_watch_create_and_list() {
        let mut wm = WatchManager::new();

        wm.add_rule(WatchRule {
            id: "watch-1".to_string(),
            domain: "amazon.com".to_string(),
            model_type: Some("Product".to_string()),
            feature_dim: 48,
            condition: WatchCondition::ValueBelow(50.0),
            notify: NotifyTarget::EventBus,
            active: true,
            created_at: Utc::now(),
            last_triggered: None,
        });

        wm.add_rule(WatchRule {
            id: "watch-2".to_string(),
            domain: "amazon.com".to_string(),
            model_type: Some("Product".to_string()),
            feature_dim: 48,
            condition: WatchCondition::ValueAbove(1000.0),
            notify: NotifyTarget::EventBus,
            active: true,
            created_at: Utc::now(),
            last_triggered: None,
        });

        let rules = wm.list_rules();
        assert_eq!(rules.len(), 2);
        assert!(rules.iter().any(|r| r.id == "watch-1"));
        assert!(rules.iter().any(|r| r.id == "watch-2"));
    }

    #[test]
    fn test_v4_watch_remove_and_verify() {
        let mut wm = WatchManager::new();

        wm.add_rule(WatchRule {
            id: "to-remove".to_string(),
            domain: "test.com".to_string(),
            model_type: Some("Product".to_string()),
            feature_dim: 48,
            condition: WatchCondition::ValueBelow(1.0),
            notify: NotifyTarget::EventBus,
            active: true,
            created_at: Utc::now(),
            last_triggered: None,
        });

        assert_eq!(wm.list_rules().len(), 1);
        wm.remove_rule("to-remove");
        assert!(wm.list_rules().is_empty());

        // Remove non-existent should not panic
        wm.remove_rule("non-existent");
    }

    #[test]
    fn test_v4_watch_unrealistic_threshold_no_trigger() {
        let mut wm = WatchManager::new();

        // Price below $0.01 — should NOT trigger for normal data
        wm.add_rule(WatchRule {
            id: "unrealistic".to_string(),
            domain: "amazon.com".to_string(),
            model_type: Some("Product".to_string()),
            feature_dim: 48,
            condition: WatchCondition::ValueBelow(0.01),
            notify: NotifyTarget::EventBus,
            active: true,
            created_at: Utc::now(),
            last_triggered: None,
        });

        let alerts = wm.evaluate("amazon.com", 48, 100.0, 95.0);
        assert!(
            alerts.is_empty(),
            "unrealistic threshold should not trigger"
        );
    }

    #[test]
    fn test_v4_watch_realistic_trigger() {
        let mut wm = WatchManager::new();

        wm.add_rule(WatchRule {
            id: "price-drop".to_string(),
            domain: "amazon.com".to_string(),
            model_type: Some("Product".to_string()),
            feature_dim: 48,
            condition: WatchCondition::ValueBelow(80.0),
            notify: NotifyTarget::EventBus,
            active: true,
            created_at: Utc::now(),
            last_triggered: None,
        });

        // Price drops to 75 — should trigger
        let alerts = wm.evaluate("amazon.com", 48, 75.0, 100.0);
        assert_eq!(alerts.len(), 1);

        // Check alert has the watch rule ID
        assert_eq!(alerts[0].rule_id, "price-drop");
    }

    #[test]
    fn test_v4_watch_recent_alerts() {
        let mut wm = WatchManager::new();

        wm.add_rule(WatchRule {
            id: "test-alert".to_string(),
            domain: "test.com".to_string(),
            model_type: Some("Product".to_string()),
            feature_dim: 48,
            condition: WatchCondition::ValueAbove(100.0),
            notify: NotifyTarget::EventBus,
            active: true,
            created_at: Utc::now(),
            last_triggered: None,
        });

        // Trigger an alert
        let alerts = wm.evaluate("test.com", 48, 150.0, 50.0);
        assert!(!alerts.is_empty());

        // Recent alerts should include it
        let recent = wm.recent_alerts(10);
        assert!(
            !recent.is_empty(),
            "recent alerts should include triggered alert"
        );
    }
}
