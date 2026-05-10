pub(super) fn body_with_local_time(body: &str, formatted_time: &str) -> String {
    let body = body.trim_end();
    if body.is_empty() {
        return format!("Time: {formatted_time}");
    }

    if let Some((details, blocks)) = split_trailing_notification_blocks_text(body) {
        let details = details.trim_end();
        if details.is_empty() {
            return format!("Time: {formatted_time}\n\n{blocks}");
        }

        return format!("{details}\nTime: {formatted_time}\n\n{blocks}");
    }

    format!("{body}\nTime: {formatted_time}")
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct NotificationBlocks<'a> {
    pub(super) prompt: Option<&'a str>,
    pub(super) answer: Option<AnswerBlock<'a>>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct AnswerBlock<'a> {
    pub(super) label: &'static str,
    pub(super) content: &'a str,
}

pub(super) fn split_trailing_notification_blocks(
    body: &str,
) -> Option<(&str, NotificationBlocks<'_>)> {
    let (details, blocks) = split_trailing_notification_blocks_text(body)?;
    Some((details, parse_notification_blocks(blocks)))
}

fn split_trailing_notification_blocks_text(body: &str) -> Option<(&str, &str)> {
    let (index, content_index, _) = find_block_start(body, &["Prompt", "Preview", "Answer"])?;
    Some((&body[..index], body[content_index..].trim_start()))
}

fn parse_notification_blocks(blocks: &str) -> NotificationBlocks<'_> {
    if let Some(prompt) = blocks.strip_prefix("Prompt:") {
        let prompt = prompt.trim_start();
        if let Some((answer_index, answer_content_index, answer_label)) =
            find_block_start(prompt, &["Preview", "Answer"])
        {
            let answer_prefix = format!("{answer_label}:");
            let answer_content_start = answer_content_index + answer_prefix.len();
            return NotificationBlocks {
                prompt: present(&prompt[..answer_index]),
                answer: Some(AnswerBlock {
                    label: answer_label,
                    content: prompt[answer_content_start..].trim_start(),
                }),
            };
        }

        return NotificationBlocks {
            prompt: present(prompt),
            answer: None,
        };
    }

    for label in ["Preview", "Answer"] {
        let prefix = format!("{label}:");
        if let Some(content) = blocks.strip_prefix(&prefix) {
            return NotificationBlocks {
                prompt: None,
                answer: Some(AnswerBlock {
                    label,
                    content: content.trim_start(),
                }),
            };
        }
    }

    NotificationBlocks {
        prompt: None,
        answer: None,
    }
}

fn find_block_start(body: &str, labels: &[&'static str]) -> Option<(usize, usize, &'static str)> {
    let mut candidate = None;

    for label in labels {
        let start_prefix = format!("{label}:");
        if body.starts_with(&start_prefix) {
            candidate = Some((0, 0, *label));
        }

        let block_prefix = format!("\n\n{label}:");
        if let Some(index) = body.find(&block_prefix) {
            let content_index = index + 2;
            if candidate
                .as_ref()
                .is_none_or(|(current_index, _, _)| index < *current_index)
            {
                candidate = Some((index, content_index, *label));
            }
        }
    }

    candidate
}

fn present(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_time_to_regular_body() {
        assert_eq!(
            body_with_local_time("Ready for review.", "2026-05-10 10:31:43 +08:00"),
            "Ready for review.\nTime: 2026-05-10 10:31:43 +08:00"
        );
    }

    #[test]
    fn places_time_before_trailing_preview_block() {
        assert_eq!(
            body_with_local_time(
                "Project: agents-notifier\nSession: agents-notifier sync report\nDuration: 2m 29s\nBranch: main\n\nPreview: The final answer preview...",
                "2026-05-10 10:31:43 +08:00",
            ),
            "Project: agents-notifier\nSession: agents-notifier sync report\nDuration: 2m 29s\nBranch: main\nTime: 2026-05-10 10:31:43 +08:00\n\nPreview: The final answer preview..."
        );
    }

    #[test]
    fn handles_preview_only_body() {
        assert_eq!(
            body_with_local_time(
                "Preview: The final answer preview...",
                "2026-05-10 10:31:43 +08:00"
            ),
            "Time: 2026-05-10 10:31:43 +08:00\n\nPreview: The final answer preview..."
        );
    }

    #[test]
    fn places_time_before_trailing_answer_block() {
        assert_eq!(
            body_with_local_time(
                "Project: agents-notifier\nBranch: main\n\nAnswer: Fixed the route.\n\nRun tests next.",
                "2026-05-10 10:31:43 +08:00",
            ),
            "Project: agents-notifier\nBranch: main\nTime: 2026-05-10 10:31:43 +08:00\n\nAnswer: Fixed the route.\n\nRun tests next."
        );
    }

    #[test]
    fn places_time_before_trailing_prompt_and_answer_blocks() {
        assert_eq!(
            body_with_local_time(
                "Project: agents-notifier\nBranch: main\n\nPrompt: Fix the route.\n\nAnswer: Fixed the route.",
                "2026-05-10 10:31:43 +08:00",
            ),
            "Project: agents-notifier\nBranch: main\nTime: 2026-05-10 10:31:43 +08:00\n\nPrompt: Fix the route.\n\nAnswer: Fixed the route."
        );
    }

    #[test]
    fn splits_trailing_prompt_and_answer_blocks() {
        let (details, blocks) = split_trailing_notification_blocks(
            "Project: agents-notifier\n\nPrompt: Fix the route.\n\nAnswer: Fixed the route.",
        )
        .expect("blocks should be present");

        assert_eq!(details, "Project: agents-notifier");
        assert_eq!(blocks.prompt, Some("Fix the route."));
        assert_eq!(
            blocks.answer,
            Some(AnswerBlock {
                label: "Answer",
                content: "Fixed the route.",
            })
        );
    }

    #[test]
    fn keeps_preview_text_inside_answer_block() {
        assert_eq!(
            body_with_local_time(
                "Project: agents-notifier\n\nAnswer: Full answer.\n\nPreview: this is quoted text.",
                "2026-05-10 10:31:43 +08:00",
            ),
            "Project: agents-notifier\nTime: 2026-05-10 10:31:43 +08:00\n\nAnswer: Full answer.\n\nPreview: this is quoted text."
        );
    }
}
