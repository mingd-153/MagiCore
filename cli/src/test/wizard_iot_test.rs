use super::{IotWizard, QuestionKind};

#[test]
fn wizard_board_choices_match_the_shared_registry() {
    let tree = IotWizard::build_tree();
    let QuestionKind::Select { options } = tree.kind else {
        panic!("IoT wizard root must select a framework");
    };

    for framework in options {
        let QuestionKind::Select { options: boards } = &framework.next_questions[0].kind else {
            panic!("each IoT framework must select from board choices");
        };
        let registered = mgc_iot_adapter::boards_for_framework(&framework.value);
        let registered_ids = registered
            .iter()
            .map(|board| board.id.as_str())
            .collect::<Vec<_>>();
        let wizard_ids = boards
            .iter()
            .map(|board| board.value.as_str())
            .collect::<Vec<_>>();
        assert_eq!(wizard_ids, registered_ids);
    }
}

#[test]
fn each_framework_default_board_is_registered_for_that_framework() {
    for framework in ["esp32-rust", "platformio", "zephyr-arm"] {
        let board = IotWizard::default_board(framework);
        assert!(
            mgc_iot_adapter::board_target_for_framework(framework, &board).is_some(),
            "default board {board} must be registered for {framework}"
        );
    }
}
