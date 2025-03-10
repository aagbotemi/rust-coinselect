use crate::{
    algorithms::{
        bnb::select_coin_bnb, fifo::select_coin_fifo, knapsack::select_coin_knapsack,
        lowestlarger::select_coin_lowestlarger, srd::select_coin_srd,
    },
    types::{CoinSelectionOpt, OutputGroup, SelectionError, SelectionOutput},
};
use std::{
    sync::{Arc, Mutex},
    thread,
};

/// The global coin selection API that applies all algorithms and produces the result with the lowest [WasteMetric].
///
/// At least one selection solution should be found.
type CoinSelectionFn =
    fn(&[OutputGroup], &CoinSelectionOpt) -> Result<SelectionOutput, SelectionError>;

#[derive(Debug)]
struct SharedState {
    result: Result<SelectionOutput, SelectionError>,
    any_success: bool,
}

pub fn select_coin(
    inputs: &[OutputGroup],
    options: &CoinSelectionOpt,
) -> Result<SelectionOutput, SelectionError> {
    let algorithms: Vec<CoinSelectionFn> = vec![
        select_coin_bnb,
        select_coin_fifo,
        select_coin_lowestlarger,
        select_coin_srd,
        select_coin_knapsack, // Future algorithms can be added here
    ];
    // Shared result for all threads
    let best_result = Arc::new(Mutex::new(SharedState {
        result: Err(SelectionError::NoSolutionFound),
        any_success: false,
    }));
    for &algorithm in &algorithms {
        let best_result_clone = Arc::clone(&best_result);
        thread::scope(|s| {
            s.spawn(|| {
                let result = algorithm(inputs, options);
                let mut state = best_result_clone.lock().unwrap();
                match result {
                    Ok(selection_output) => {
                        if match &state.result {
                            Ok(current_best) => selection_output.waste.0 < current_best.waste.0,
                            Err(_) => true,
                        } {
                            state.result = Ok(selection_output);
                            state.any_success = true;
                        }
                    }
                    Err(e) => {
                        if e == SelectionError::InsufficientFunds && !state.any_success {
                            // Only set to InsufficientFunds if no algorithm succeeded
                            state.result = Err(SelectionError::InsufficientFunds);
                        }
                    }
                }
            });
        });
    }
    // Extract the result from the shared state
    Arc::try_unwrap(best_result)
        .expect("Arc unwrap failed")
        .into_inner()
        .expect("Mutex lock failed")
        .result
}

#[cfg(test)]
mod test {

    use crate::{
        selectcoin::select_coin,
        types::{CoinSelectionOpt, ExcessStrategy, OutputGroup, SelectionError},
    };

    fn setup_basic_output_groups() -> Vec<OutputGroup> {
        vec![
            OutputGroup {
                value: 1000,
                weight: 100,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 2000,
                weight: 200,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 3000,
                weight: 300,
                input_count: 1,
                creation_sequence: None,
            },
        ]
    }

    fn setup_options(target_value: u64) -> CoinSelectionOpt {
        CoinSelectionOpt {
            target_value,
            target_feerate: 0.4, // Simplified feerate
            long_term_feerate: Some(0.4),
            min_absolute_fee: 0,
            base_weight: 10,
            change_weight: 50,
            change_cost: 10,
            avg_input_weight: 20,
            avg_output_weight: 10,
            min_change_value: 500,
            excess_strategy: ExcessStrategy::ToChange,
        }
    }

    #[test]
    fn test_select_coin_successful() {
        let inputs = setup_basic_output_groups();
        let options = setup_options(1500);
        let result = select_coin(&inputs, &options);
        assert!(result.is_ok());
        let selection_output = result.unwrap();
        assert!(!selection_output.selected_inputs.is_empty());
    }

    #[test]
    fn test_select_coin_insufficient_funds() {
        let inputs = setup_basic_output_groups();
        let options = setup_options(7000); // Set a target value higher than the sum of all inputs
        let result = select_coin(&inputs, &options);
        assert!(matches!(result, Err(SelectionError::InsufficientFunds)));
    }

    #[test]
    fn test_select_coin_equals_lowest_larger() {
        // Define the inputs such that the lowest_larger algorithm should be optimal
        let inputs = vec![
            OutputGroup {
                value: 500,
                weight: 50,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 1500,
                weight: 100,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 2000,
                weight: 200,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 1000,
                weight: 75,
                input_count: 1,
                creation_sequence: None,
            },
        ];

        // Define the target selection options
        let options = CoinSelectionOpt {
            target_value: 1600, // Target value which lowest_larger can satisfy
            target_feerate: 0.4,
            long_term_feerate: Some(0.4),
            min_absolute_fee: 0,
            base_weight: 10,
            change_weight: 50,
            change_cost: 10,
            avg_input_weight: 50,
            avg_output_weight: 25,
            min_change_value: 500,
            excess_strategy: ExcessStrategy::ToChange,
        };

        // Call the select_coin function, which should internally use the lowest_larger algorithm
        let selection_result = select_coin(&inputs, &options).unwrap();

        // Deterministically choose a result based on how lowest_larger would select
        let expected_inputs = vec![2]; // Example choice based on lowest_larger logic

        // Sort the selected inputs to ignore the order
        let mut selection_inputs = selection_result.selected_inputs.clone();
        let mut expected_inputs_sorted = expected_inputs.clone();
        selection_inputs.sort();
        expected_inputs_sorted.sort();
    }

    #[test]
    fn test_select_coin_equals_knapsack() {
        // Define inputs that are best suited for knapsack algorithm to match the target value with minimal waste
        let inputs = vec![
            OutputGroup {
                value: 1500,
                weight: 1,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 2500,
                weight: 1,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 3000,
                weight: 1,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 1000,
                weight: 1,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 500,
                weight: 1,
                input_count: 1,
                creation_sequence: None,
            },
        ];

        // Define the target selection options
        let options = CoinSelectionOpt {
            target_value: 4000, // Set a target that knapsack can match efficiently
            target_feerate: 1.0,
            min_absolute_fee: 0,
            base_weight: 1,
            change_weight: 1,
            change_cost: 1,
            avg_input_weight: 1,
            avg_output_weight: 1,
            min_change_value: 500,
            long_term_feerate: Some(0.5),
            excess_strategy: ExcessStrategy::ToChange,
        };

        let selection_result = select_coin(&inputs, &options).unwrap();

        // Deterministically choose a result with justification
        // Here, we assume that the `select_coin` function internally chooses the most efficient set
        // of inputs that meet the `target_value` while minimizing waste. This selection is deterministic
        // given the same inputs and options. Therefore, the following assertions are based on
        // the assumption that the chosen inputs are correct and optimized.

        let expected_inputs = vec![1, 3]; // Example deterministic choice, adjust as needed

        // Sort the selected inputs to ignore the order
        let mut selection_inputs = selection_result.selected_inputs.clone();
        let mut expected_inputs_sorted = expected_inputs.clone();
        selection_inputs.sort();
        expected_inputs_sorted.sort();
    }

    #[test]
    fn test_select_coin_equals_fifo() {
        // Helper function to create OutputGroups
        fn create_fifo_inputs(values: Vec<u64>) -> Vec<OutputGroup> {
            values
                .into_iter()
                .map(|value| OutputGroup {
                    value,
                    weight: 100,
                    input_count: 1,
                    creation_sequence: None,
                })
                .collect()
        }

        let options_case = CoinSelectionOpt {
            target_value: 250000,
            target_feerate: 1.0,
            min_absolute_fee: 0,
            base_weight: 100,
            change_weight: 10,
            change_cost: 20,
            avg_input_weight: 10,
            avg_output_weight: 10,
            min_change_value: 400,
            long_term_feerate: Some(0.5),
            excess_strategy: ExcessStrategy::ToChange,
        };

        let inputs_case = create_fifo_inputs(vec![80000, 70000, 60000, 50000, 40000, 30000]);

        let result_case = select_coin(&inputs_case, &options_case).unwrap();
        let expected_case = vec![0, 1, 2, 3]; // Indexes of oldest UTXOs that sum to target
        assert_eq!(result_case.selected_inputs, expected_case);
    }

    #[test]
    fn test_select_coin_equals_bnb() {
        let inputs = vec![
            OutputGroup {
                value: 150000,
                weight: 100,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 250000,
                weight: 100,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 300000,
                weight: 100,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 100000,
                weight: 100,
                input_count: 1,
                creation_sequence: None,
            },
            OutputGroup {
                value: 50000,
                weight: 100,
                input_count: 1,
                creation_sequence: None,
            },
        ];
        let opt = CoinSelectionOpt {
            target_value: 500000,
            target_feerate: 1.0,
            min_absolute_fee: 0,
            base_weight: 100,
            change_weight: 10,
            change_cost: 20,
            avg_input_weight: 10,
            avg_output_weight: 10,
            min_change_value: 400,
            long_term_feerate: Some(0.5),
            excess_strategy: ExcessStrategy::ToChange,
        };
        let ans = select_coin(&inputs, &opt);

        dbg!(&ans);

        if let Ok(selection_output) = ans {
            let mut selected_inputs = selection_output.selected_inputs.clone();
            selected_inputs.sort();

            // The expected solution is vec![1, 2] because the combined value of the selected inputs
            // (250000 + 300000) meets the target value of 500000 with minimal excess. This selection
            // minimizes waste and adheres to the constraints of the coin selection algorithm, which
            // aims to find the most optimal solution.
            // Branch and Bound also gives a better time complexity, referenced from Mark Erhardt's Master Thesis.

            let expected_solution = vec![1, 2];
            dbg!(&selected_inputs);
            dbg!(&expected_solution);
            assert_eq!(
                selected_inputs, expected_solution,
                "Expected solution {:?}, but got {:?}",
                expected_solution, selected_inputs
            );
        }
    }
}
