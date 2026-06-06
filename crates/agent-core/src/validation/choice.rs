//! `Choice_Prompt` answer validation (R4.5, R4.6) and recommendation validation (R5.5).

use crate::models::{ChoiceAnswer, ChoicePrompt, ErrorCode, ErrorPayload};

/// Validates a user's answer against the given `ChoicePrompt`.
///
/// Returns `Ok(())` iff ALL of the following hold:
/// 1. All selected options exist in the prompt's option list.
/// 2. If `prompt.multi_select == false`, at most one option is selected.
/// 3. Custom text is only provided when `prompt.allow_custom_text == true`.
/// 4. If no options are selected, custom text must be provided.
///
/// Otherwise returns `Err(ErrorPayload)` with `ErrorCode::InvalidChoice`.
pub fn validate_choice_answer(
    prompt: &ChoicePrompt,
    answer: &ChoiceAnswer,
) -> Result<(), ErrorPayload> {
    // Rule 1: answer.options ⊆ prompt.options
    let valid_ids: Vec<&str> = prompt.options.iter().map(|o| o.option_id.as_str()).collect();
    let all_exist = answer.options.iter().all(|id| valid_ids.contains(&id.as_str()));
    if !all_exist {
        return Err(invalid_choice_error());
    }

    // Rule 2: single-select constraint
    if !prompt.multi_select && answer.options.len() > 1 {
        return Err(invalid_choice_error());
    }

    // Rule 3: custom text permission
    if answer.custom_text.is_some() && !prompt.allow_custom_text {
        return Err(invalid_choice_error());
    }

    // Rule 4: empty options require custom text
    if answer.options.is_empty() && answer.custom_text.is_none() {
        return Err(invalid_choice_error());
    }

    Ok(())
}

/// Validates that a `ChoicePrompt`'s recommendation is consistent with its options (Property 9).
///
/// If `prompt.recommendation` is `Some(rec_id)`, then there must exist an option in
/// `prompt.options` whose `option_id == rec_id` **and** whose `explanation` is `Some(_)`.
///
/// Returns `Ok(())` if `recommendation` is `None` or the above condition holds.
/// Returns `Err(ErrorPayload)` with `ErrorCode::InvalidChoice` otherwise.
pub fn validate_recommendation(prompt: &ChoicePrompt) -> Result<(), ErrorPayload> {
    if let Some(ref rec_id) = prompt.recommendation {
        let valid = prompt.options.iter().any(|o| {
            o.option_id == *rec_id && o.explanation.is_some()
        });
        if !valid {
            return Err(ErrorPayload {
                error_code: ErrorCode::InvalidChoice,
                message: "无效的选项，请从选项中选择".to_string(),
                details: None,
            });
        }
    }
    Ok(())
}

fn invalid_choice_error() -> ErrorPayload {
    ErrorPayload {
        error_code: ErrorCode::InvalidChoice,
        message: "无效的选项，请从选项中选择".to_string(),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ChoiceOption;
    use uuid::Uuid;

    fn make_prompt(
        options: Vec<(&str, &str, Option<&str>)>,
        multi_select: bool,
        allow_custom_text: bool,
        recommendation: Option<&str>,
    ) -> ChoicePrompt {
        ChoicePrompt {
            prompt_id: Uuid::new_v4(),
            question: "测试问题".to_string(),
            options: options
                .into_iter()
                .map(|(id, text, expl)| ChoiceOption {
                    option_id: id.to_string(),
                    text: text.to_string(),
                    explanation: expl.map(std::string::ToString::to_string),
                })
                .collect(),
            multi_select,
            allow_custom_text,
            recommendation: recommendation.map(std::string::ToString::to_string),
        }
    }

    fn make_answer(options: Vec<&str>, custom_text: Option<&str>) -> ChoiceAnswer {
        ChoiceAnswer {
            prompt_id: Uuid::new_v4(),
            options: options.into_iter().map(std::string::ToString::to_string).collect(),
            custom_text: custom_text.map(std::string::ToString::to_string),
        }
    }

    // --- Rule 1: answer.options ⊆ prompt.options ---

    #[test]
    fn rule1_valid_option_subset() {
        let prompt = make_prompt(
            vec![("a", "选项A", None), ("b", "选项B", None)],
            true,
            false,
            None,
        );
        let answer = make_answer(vec!["a", "b"], None);
        assert!(validate_choice_answer(&prompt, &answer).is_ok());
    }

    #[test]
    fn rule1_invalid_option_not_in_prompt() {
        let prompt = make_prompt(
            vec![("a", "选项A", None), ("b", "选项B", None)],
            true,
            false,
            None,
        );
        let answer = make_answer(vec!["a", "c"], None);
        let err = validate_choice_answer(&prompt, &answer).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::InvalidChoice);
    }

    #[test]
    fn rule1_all_invalid_options() {
        let prompt = make_prompt(
            vec![("a", "选项A", None)],
            true,
            false,
            None,
        );
        let answer = make_answer(vec!["x", "y"], None);
        let err = validate_choice_answer(&prompt, &answer).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::InvalidChoice);
    }

    // --- Rule 2: single-select constraint ---

    #[test]
    fn rule2_single_select_one_option_ok() {
        let prompt = make_prompt(
            vec![("a", "选项A", None), ("b", "选项B", None)],
            false,
            false,
            None,
        );
        let answer = make_answer(vec!["a"], None);
        assert!(validate_choice_answer(&prompt, &answer).is_ok());
    }

    #[test]
    fn rule2_single_select_multiple_options_err() {
        let prompt = make_prompt(
            vec![("a", "选项A", None), ("b", "选项B", None)],
            false,
            false,
            None,
        );
        let answer = make_answer(vec!["a", "b"], None);
        let err = validate_choice_answer(&prompt, &answer).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::InvalidChoice);
    }

    #[test]
    fn rule2_multi_select_multiple_options_ok() {
        let prompt = make_prompt(
            vec![("a", "选项A", None), ("b", "选项B", None), ("c", "选项C", None)],
            true,
            false,
            None,
        );
        let answer = make_answer(vec!["a", "b", "c"], None);
        assert!(validate_choice_answer(&prompt, &answer).is_ok());
    }

    // --- Rule 3: custom text permission ---

    #[test]
    fn rule3_custom_text_allowed() {
        let prompt = make_prompt(
            vec![("a", "选项A", None)],
            false,
            true,
            None,
        );
        let answer = make_answer(vec!["a"], Some("自定义内容"));
        assert!(validate_choice_answer(&prompt, &answer).is_ok());
    }

    #[test]
    fn rule3_custom_text_not_allowed() {
        let prompt = make_prompt(
            vec![("a", "选项A", None)],
            false,
            false,
            None,
        );
        let answer = make_answer(vec!["a"], Some("自定义内容"));
        let err = validate_choice_answer(&prompt, &answer).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::InvalidChoice);
    }

    // --- Rule 4: empty options require custom text ---

    #[test]
    fn rule4_empty_options_with_custom_text_ok() {
        let prompt = make_prompt(
            vec![("a", "选项A", None)],
            false,
            true,
            None,
        );
        let answer = make_answer(vec![], Some("用户自定义回答"));
        assert!(validate_choice_answer(&prompt, &answer).is_ok());
    }

    #[test]
    fn rule4_empty_options_without_custom_text_err() {
        let prompt = make_prompt(
            vec![("a", "选项A", None)],
            false,
            true,
            None,
        );
        let answer = make_answer(vec![], None);
        let err = validate_choice_answer(&prompt, &answer).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::InvalidChoice);
    }

    // --- Error message format ---

    #[test]
    fn error_message_is_chinese() {
        let prompt = make_prompt(vec![("a", "选项A", None)], false, false, None);
        let answer = make_answer(vec!["x"], None);
        let err = validate_choice_answer(&prompt, &answer).unwrap_err();
        assert_eq!(err.message, "无效的选项，请从选项中选择");
    }

    // --- validate_recommendation (Property 9) ---

    #[test]
    fn recommendation_none_is_valid() {
        let prompt = make_prompt(
            vec![("a", "选项A", None)],
            false,
            false,
            None,
        );
        assert!(validate_recommendation(&prompt).is_ok());
    }

    #[test]
    fn recommendation_present_in_options_with_explanation() {
        let prompt = make_prompt(
            vec![("a", "选项A", Some("推荐理由")), ("b", "选项B", None)],
            false,
            false,
            Some("a"),
        );
        assert!(validate_recommendation(&prompt).is_ok());
    }

    #[test]
    fn recommendation_present_but_no_explanation() {
        let prompt = make_prompt(
            vec![("a", "选项A", None), ("b", "选项B", None)],
            false,
            false,
            Some("a"),
        );
        let err = validate_recommendation(&prompt).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::InvalidChoice);
    }

    #[test]
    fn recommendation_not_in_options() {
        let prompt = make_prompt(
            vec![("a", "选项A", Some("理由")), ("b", "选项B", None)],
            false,
            false,
            Some("c"),
        );
        let err = validate_recommendation(&prompt).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::InvalidChoice);
    }

    #[test]
    fn recommendation_in_options_without_explanation_fails() {
        // recommendation points to an option that exists but has no explanation
        let prompt = make_prompt(
            vec![("a", "选项A", Some("有理由")), ("b", "选项B", None)],
            false,
            false,
            Some("b"),
        );
        let err = validate_recommendation(&prompt).unwrap_err();
        assert_eq!(err.error_code, ErrorCode::InvalidChoice);
    }
}
