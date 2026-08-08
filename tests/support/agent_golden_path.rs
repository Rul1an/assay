use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    static DRIVEN_OUTCOMES: RefCell<BTreeMap<(String, String), usize>> =
        const { RefCell::new(BTreeMap::new()) };
}

pub fn record_outcome(step_id: &str, outcome_name: &str) {
    DRIVEN_OUTCOMES.with(|driven| {
        *driven
            .borrow_mut()
            .entry((step_id.to_owned(), outcome_name.to_owned()))
            .or_default() += 1;
    });
}

pub fn assert_exact(contract: &Value, binary: &str, scenarios: &[fn()]) {
    DRIVEN_OUTCOMES.with(|driven| driven.borrow_mut().clear());
    for scenario in scenarios {
        scenario();
    }
    let driven = DRIVEN_OUTCOMES.with(|driven| driven.borrow().clone());
    assert_eq!(
        driven,
        expected_contract_outcomes(contract, binary),
        "runtime scenarios must drive every contract outcome exactly once"
    );
}

fn expected_contract_outcomes(contract: &Value, binary: &str) -> BTreeMap<(String, String), usize> {
    let mut expected = BTreeMap::new();
    for step in contract["steps"]
        .as_array()
        .expect("contract steps array")
        .iter()
        .filter(|step| step["binary"] == binary)
    {
        let step_id = step["id"].as_str().expect("contract step id string");
        for outcome in step["outcomes"].as_array().expect("step outcomes array") {
            let outcome_name = outcome["name"]
                .as_str()
                .expect("contract outcome name string");
            assert!(
                expected
                    .insert((step_id.to_owned(), outcome_name.to_owned()), 1)
                    .is_none(),
                "duplicate contract outcome {step_id}/{outcome_name}"
            );
        }
    }
    expected
}
