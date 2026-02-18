//! Encode extracted actions into ActionRecord OpCodes.

use crate::map::types::{ActionRecord, OpCode};

/// An action extracted from the DOM by the actions.ts extractor.
#[derive(Debug, Clone)]
pub struct ExtractedAction {
    pub label: String,
    pub element_type: String,
    pub opcode_hint: Option<u16>,
    pub risk: u8,
    pub target_url: Option<String>,
}

/// Encode a list of extracted actions into ActionRecords.
pub fn encode_actions(actions: &[ExtractedAction]) -> Vec<ActionRecord> {
    actions.iter().map(encode_single_action).collect()
}

/// Encode extracted actions from JSON (as produced by actions.ts extractor).
pub fn encode_actions_from_json(json_actions: &serde_json::Value) -> Vec<ActionRecord> {
    let Some(arr) = json_actions.as_array() else {
        return Vec::new();
    };

    arr.iter()
        .map(|item| {
            let opcode_val = item.get("opcode").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let risk = item.get("risk").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
            let cost = item.get("cost").and_then(|v| v.as_u64()).unwrap_or(0) as u8;

            ActionRecord {
                opcode: OpCode::from_u16(opcode_val),
                target_node: -2, // Unknown at extraction time
                cost_hint: cost,
                risk,
                http_executable: false,
            }
        })
        .collect()
}

fn encode_single_action(action: &ExtractedAction) -> ActionRecord {
    let opcode = if let Some(hint) = action.opcode_hint {
        OpCode::from_u16(hint)
    } else {
        classify_action_opcode(&action.label, &action.element_type)
    };

    ActionRecord {
        opcode,
        target_node: -2, // Unknown until resolved
        cost_hint: 0,
        risk: action.risk,
        http_executable: false,
    }
}

/// Classify an action into an OpCode based on label text and element type.
fn classify_action_opcode(label: &str, element_type: &str) -> OpCode {
    let label_lower = label.to_lowercase();

    // Commerce actions (category 0x02)
    if label_lower.contains("add to cart")
        || label_lower.contains("add to bag")
        || label_lower.contains("buy now")
    {
        return OpCode::new(0x02, 0x00); // add_to_cart
    }
    if label_lower.contains("remove") && label_lower.contains("cart") {
        return OpCode::new(0x02, 0x01); // remove_from_cart
    }
    if label_lower.contains("checkout") || label_lower.contains("check out") {
        return OpCode::new(0x02, 0x03); // checkout
    }
    if label_lower.contains("wishlist") || label_lower.contains("save for later") {
        return OpCode::new(0x02, 0x05); // add_to_wishlist
    }
    if label_lower.contains("apply") && label_lower.contains("coupon") {
        return OpCode::new(0x02, 0x04); // apply_coupon
    }

    // Form actions (category 0x03)
    if label_lower.contains("submit") || element_type == "submit" {
        return OpCode::new(0x03, 0x05); // submit_form
    }
    if label_lower.contains("search") || element_type == "search" {
        return OpCode::new(0x01, 0x00); // search
    }
    if label_lower.contains("filter") || label_lower.contains("sort") {
        return OpCode::new(0x01, 0x01); // filter
    }

    // Auth actions (category 0x04)
    if label_lower.contains("log in")
        || label_lower.contains("login")
        || label_lower.contains("sign in")
    {
        return OpCode::new(0x04, 0x00); // login
    }
    if label_lower.contains("sign up") || label_lower.contains("register") {
        return OpCode::new(0x04, 0x02); // register
    }
    if label_lower.contains("log out") || label_lower.contains("logout") {
        return OpCode::new(0x04, 0x01); // logout
    }

    // Media actions (category 0x05)
    if label_lower.contains("play") {
        return OpCode::new(0x05, 0x00); // play
    }
    if label_lower.contains("download") {
        return OpCode::new(0x05, 0x03); // download
    }

    // Social actions (category 0x06)
    if label_lower.contains("share") {
        return OpCode::new(0x06, 0x00); // share
    }
    if label_lower.contains("like") || label_lower.contains("upvote") {
        return OpCode::new(0x06, 0x01); // like
    }
    if label_lower.contains("comment") {
        return OpCode::new(0x06, 0x02); // comment
    }

    // Navigation (category 0x00) - default
    if element_type == "a" || element_type == "link" {
        return OpCode::new(0x00, 0x00); // click_link
    }

    OpCode::new(0x00, 0x01) // generic click
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_add_to_cart() {
        let op = classify_action_opcode("Add to Cart", "button");
        assert_eq!(op.category, 0x02);
        assert_eq!(op.action, 0x00);
    }

    #[test]
    fn test_classify_login() {
        let op = classify_action_opcode("Sign In", "button");
        assert_eq!(op.category, 0x04);
        assert_eq!(op.action, 0x00);
    }

    #[test]
    fn test_classify_submit() {
        let op = classify_action_opcode("Submit", "submit");
        assert_eq!(op.category, 0x03);
        assert_eq!(op.action, 0x05);
    }

    #[test]
    fn test_encode_actions_from_json() {
        let json = serde_json::json!([
            {"opcode": 0x0200, "risk": 1, "cost": 0},
            {"opcode": 0x0400, "risk": 0, "cost": 0}
        ]);
        let records = encode_actions_from_json(&json);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].opcode.category, 0x02);
        assert_eq!(records[0].opcode.action, 0x00);
        assert_eq!(records[0].risk, 1);
        assert_eq!(records[1].opcode.category, 0x04);
    }
}
