pub(super) fn body_with_local_time(body: &str, formatted_time: &str) -> String {
    let body = body.trim_end();
    if body.is_empty() {
        return format!("Time: {formatted_time}");
    }

    if let Some((details, block)) = split_trailing_answer_block(body) {
        let details = details.trim_end();
        if details.is_empty() {
            return format!(
                "Time: {formatted_time}\n\n{}: {}",
                block.label, block.content
            );
        }

        return format!(
            "{details}\nTime: {formatted_time}\n\n{}: {}",
            block.label, block.content
        );
    }

    format!("{body}\nTime: {formatted_time}")
}

pub(super) struct AnswerBlock<'a> {
    pub(super) label: &'static str,
    pub(super) content: &'a str,
}

pub(super) fn split_trailing_answer_block(body: &str) -> Option<(&str, AnswerBlock<'_>)> {
    for label in ["Preview", "Answer"] {
        let prefix = format!("{label}:");
        if let Some(content) = body.strip_prefix(&prefix) {
            return Some((
                "",
                AnswerBlock {
                    label,
                    content: content.trim_start(),
                },
            ));
        }
    }

    let mut candidate = None;
    for label in ["Preview", "Answer"] {
        let block_prefix = format!("\n\n{label}:");
        if let Some(index) = body.find(&block_prefix) {
            let content_index = index + block_prefix.len();
            if candidate
                .as_ref()
                .is_none_or(|(current_index, _, _)| index < *current_index)
            {
                candidate = Some((index, content_index, label));
            }
        }
    }

    candidate.map(|(index, content_index, label)| {
        (
            &body[..index],
            AnswerBlock {
                label,
                content: body[content_index..].trim_start(),
            },
        )
    })
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
